//! Builds a dynamic GraphQL schema from resource definitions and app state (M15).

use std::sync::Arc;

use async_graphql::dynamic::{
    Field, FieldFuture, InputObject, InputValue, Object, Schema, SchemaBuilder, Subscription,
    SubscriptionField, SubscriptionFieldFuture, TypeRef,
};
use async_graphql::Value;
use shaperail_core::{
    EndpointSpec, FieldType, GraphQLConfig, HttpMethod, PaginationStyle, RelationType,
    ResourceDefinition, ShaperailError,
};

use crate::auth::rbac;
use crate::db::{FilterSet, PageRequest, ResourceQuery, SortParam};
use crate::handlers::crud::{
    extract_input_from_value, run_write_side_effects, schedule_file_cleanup, store_for_or_error,
    AppState,
};
use crate::handlers::validate::validate_input;

/// Context passed into GraphQL resolvers (state, resources, auth).
#[derive(Clone)]
pub struct GqlContext {
    pub state: Arc<AppState>,
    pub resources: Vec<ResourceDefinition>,
    /// Authenticated user from JWT/API key (same as REST).
    pub user: Option<crate::auth::extractor::AuthenticatedUser>,
    /// DataLoader for batching/caching relation lookups (N+1 prevention).
    pub loader: super::dataloader::RelationLoader,
}

/// Type alias for the dynamic schema (for clarity at call sites).
pub type GraphQLSchema = Schema;

/// Returns TypeRef for schema fields. Uses only nullable refs so the dynamic
/// schema resolves base type names (e.g. "String"); named_nn causes lookup of "String!" which fails.
fn field_type_to_type_ref(ft: &FieldType, _required: bool) -> TypeRef {
    match ft {
        FieldType::Uuid => TypeRef::named("String"),
        FieldType::String | FieldType::Enum | FieldType::File => TypeRef::named("String"),
        FieldType::Integer => TypeRef::named("Int"),
        FieldType::Bigint => TypeRef::named("Int"),
        FieldType::Number => TypeRef::named("Float"),
        FieldType::Boolean => TypeRef::named("Boolean"),
        FieldType::Timestamp | FieldType::Date => TypeRef::named("String"),
        FieldType::Json | FieldType::Array => TypeRef::named("String"),
    }
}

/// Converts serde_json::Value to async_graphql::Value for resolver results.
fn json_to_gql_value(v: &serde_json::Value) -> Value {
    Value::from_json(v.clone()).unwrap_or(Value::Null)
}

/// Pascal-case resource name for GraphQL type (e.g. "posts" -> "Post").
fn object_type_name(resource: &str) -> String {
    let mut s = resource.to_string();
    if let Some(r) = s.get_mut(0..1) {
        r.make_ascii_uppercase();
    }
    s
}

/// Builds the Query object with list and get fields for each resource.
fn build_query_object(resources: &[ResourceDefinition]) -> Object {
    let mut query = Object::new("Query");

    for resource in resources {
        let type_name = object_type_name(&resource.resource);
        let list_type = TypeRef::named_list_nn(type_name.clone());
        let single_type = TypeRef::named(type_name.clone());

        // list_<resource>(limit, offset)
        let res = resource.clone();
        let list_field = Field::new(
            format!("list_{}", resource.resource),
            list_type,
            move |ctx| {
                let res = res.clone();
                FieldFuture::new(async move {
                    let gql = ctx.data::<GqlContext>().map_err(|e| e.message)?;
                    let endpoint = res
                        .endpoints
                        .as_ref()
                        .and_then(|e| e.get("list"))
                        .cloned()
                        .unwrap_or_else(|| EndpointSpec {
                            method: Some(HttpMethod::Get),
                            path: Some(format!("/{}", res.resource)),
                            auth: None,
                            input: None,
                            filters: None,
                            search: None,
                            pagination: Some(PaginationStyle::Offset),
                            sort: None,
                            cache: None,
                            controller: None,
                            events: None,
                            jobs: None,
                            upload: None,
                            soft_delete: false,
                        });
                    rbac::enforce(endpoint.auth.as_ref(), gql.user.as_ref())
                        .map_err(|e| e.to_string())?;
                    let store_opt = store_for_or_error(&gql.state, &res)?;
                    let limit = ctx
                        .args
                        .get("limit")
                        .and_then(|v| v.i64().ok())
                        .unwrap_or(25);
                    let offset = ctx
                        .args
                        .get("offset")
                        .and_then(|v| v.i64().ok())
                        .unwrap_or(0);
                    let page = PageRequest::Offset {
                        offset,
                        limit: PageRequest::clamped_limit(Some(limit)),
                    };
                    let filters = FilterSet::default();
                    let sort = SortParam::default();

                    let (rows, _meta) = if let Some(store) = store_opt {
                        store
                            .find_all(&endpoint, &filters, None, &sort, &page)
                            .await
                            .map_err(|e: ShaperailError| e.to_string())?
                    } else {
                        let rq = ResourceQuery::new(&res, &gql.state.pool);
                        rq.find_all(&filters, None, &sort, &page)
                            .await
                            .map_err(|e: ShaperailError| e.to_string())?
                    };

                    let list: Vec<Value> =
                        rows.into_iter().map(|r| json_to_gql_value(&r.0)).collect();
                    Ok(Some(Value::List(list)))
                })
            },
        )
        .argument(async_graphql::dynamic::InputValue::new(
            "limit",
            TypeRef::named("Int"),
        ))
        .argument(async_graphql::dynamic::InputValue::new(
            "offset",
            TypeRef::named("Int"),
        ));

        query = query.field(list_field);

        // <singular>(id: ID!)
        let res = resource.clone();
        let get_field = Field::new(singular_name(&resource.resource), single_type, move |ctx| {
            let res = res.clone();
            FieldFuture::new(async move {
                let id_str = ctx
                    .args
                    .get("id")
                    .and_then(|v| v.string().ok())
                    .ok_or("id required")?;
                let id = uuid::Uuid::parse_str(id_str).map_err(|e| e.to_string())?;
                let gql = ctx.data::<GqlContext>().map_err(|e| e.message)?;
                let endpoint = res
                    .endpoints
                    .as_ref()
                    .and_then(|e| e.get("get"))
                    .cloned()
                    .unwrap_or_else(|| EndpointSpec {
                        method: Some(HttpMethod::Get),
                        path: Some(format!("/{}/:id", res.resource)),
                        auth: None,
                        input: None,
                        filters: None,
                        search: None,
                        pagination: None,
                        sort: None,
                        cache: None,
                        controller: None,
                        events: None,
                        jobs: None,
                        upload: None,
                        soft_delete: false,
                    });
                rbac::enforce(endpoint.auth.as_ref(), gql.user.as_ref())
                    .map_err(|e| e.to_string())?;
                let store_opt = store_for_or_error(&gql.state, &res)?;

                let row = if let Some(store) = store_opt {
                    store
                        .find_by_id(&id)
                        .await
                        .map_err(|e: ShaperailError| e.to_string())?
                } else {
                    let rq = ResourceQuery::new(&res, &gql.state.pool);
                    rq.find_by_id(&id)
                        .await
                        .map_err(|e: ShaperailError| e.to_string())?
                };

                if rbac::needs_owner_check(endpoint.auth.as_ref(), gql.user.as_ref()) {
                    if let Some(ref u) = gql.user {
                        rbac::check_owner(u, &row.0).map_err(|e| e.to_string())?;
                    }
                }

                Ok(Some(json_to_gql_value(&row.0)))
            })
        })
        .argument(async_graphql::dynamic::InputValue::new(
            "id",
            TypeRef::named("String"),
        ));

        query = query.field(get_field);
    }

    query
}

fn singular_name(resource: &str) -> String {
    if resource.ends_with('s') && resource.len() > 1 {
        resource[..resource.len() - 1].to_string()
    } else {
        resource.to_string()
    }
}

/// Input field names for create/update (from endpoint.input or schema).
fn input_field_names(resource: &ResourceDefinition, endpoint: &EndpointSpec) -> Vec<String> {
    if let Some(input_fields) = &endpoint.input {
        return input_fields.clone();
    }
    resource
        .schema
        .iter()
        .filter(|(_, fs)| !fs.generated && !fs.primary)
        .map(|(name, _)| name.clone())
        .collect()
}

/// Builds InputObject for create/update (one per resource that has create or update endpoint).
fn build_input_objects(resources: &[ResourceDefinition]) -> Vec<InputObject> {
    let mut out = Vec::new();
    for resource in resources {
        let has_create = resource
            .endpoints
            .as_ref()
            .map(|e| e.contains_key("create"))
            .unwrap_or(false);
        let has_update = resource
            .endpoints
            .as_ref()
            .map(|e| e.contains_key("update"))
            .unwrap_or(false);
        if !has_create && !has_update {
            continue;
        }
        let endpoint = resource
            .endpoints
            .as_ref()
            .and_then(|e| e.get("create").or_else(|| e.get("update")))
            .cloned()
            .unwrap_or_else(|| EndpointSpec {
                method: Some(HttpMethod::Post),
                path: Some(format!("/{}", resource.resource)),
                auth: None,
                input: None,
                filters: None,
                search: None,
                pagination: None,
                sort: None,
                cache: None,
                controller: None,
                events: None,
                jobs: None,
                upload: None,
                soft_delete: false,
            });
        let type_name = object_type_name(&resource.resource);
        let input_name = format!("{}Input", type_name);
        let fields = input_field_names(resource, &endpoint);
        let mut input_obj = InputObject::new(input_name.clone());
        for field_name in &fields {
            if let Some(fs) = resource.schema.get(field_name) {
                let ty = field_type_to_type_ref(&fs.field_type, false);
                input_obj = input_obj.field(InputValue::new(field_name.clone(), ty));
            }
        }
        out.push(input_obj);
    }
    out
}

/// Builds the Mutation object with create, update, delete for each resource.
fn build_mutation_object(resources: &[ResourceDefinition]) -> Object {
    let mut mutation = Object::new("Mutation");

    for resource in resources {
        let type_name = object_type_name(&resource.resource);
        let single_type = TypeRef::named(type_name.clone());
        let input_type_name = format!("{}Input", type_name);

        if resource
            .endpoints
            .as_ref()
            .map(|e| e.contains_key("create"))
            .unwrap_or(false)
        {
            let res = resource.clone();
            let create_field = Field::new(
                format!("create_{}", resource.resource),
                single_type.clone(),
                move |ctx| {
                    let res = res.clone();
                    FieldFuture::new(async move {
                        let gql = ctx.data::<GqlContext>().map_err(|e| e.message)?;
                        let endpoint = res
                            .endpoints
                            .as_ref()
                            .and_then(|e| e.get("create"))
                            .cloned()
                            .ok_or("create endpoint missing")?;
                        rbac::enforce(endpoint.auth.as_ref(), gql.user.as_ref())
                            .map_err(|e| e.to_string())?;
                        let input_accessor = ctx.args.try_get("input").map_err(|e| e.message)?;
                        let json_val = input_accessor
                            .as_value()
                            .clone()
                            .into_json()
                            .map_err(|e| e.to_string())?;
                        let input_data = extract_input_from_value(&json_val, &res, &endpoint)
                            .map_err(|e| e.to_string())?;
                        validate_input(&input_data, &res).map_err(|e| e.to_string())?;
                        let store_opt = store_for_or_error(&gql.state, &res)?;
                        let row = if let Some(store) = store_opt {
                            store
                                .insert(&input_data)
                                .await
                                .map_err(|e: ShaperailError| e.to_string())?
                        } else {
                            let rq = ResourceQuery::new(&res, &gql.state.pool);
                            rq.insert(&input_data)
                                .await
                                .map_err(|e: ShaperailError| e.to_string())?
                        };
                        run_write_side_effects(&gql.state, &res, &endpoint, "created", &row.0)
                            .await;
                        Ok(Some(json_to_gql_value(&row.0)))
                    })
                },
            )
            .argument(InputValue::new(
                "input",
                TypeRef::named(input_type_name.clone()),
            ));
            mutation = mutation.field(create_field);
        }

        if resource
            .endpoints
            .as_ref()
            .map(|e| e.contains_key("update"))
            .unwrap_or(false)
        {
            let res = resource.clone();
            let update_field = Field::new(
                format!("update_{}", resource.resource),
                single_type.clone(),
                move |ctx| {
                    let res = res.clone();
                    FieldFuture::new(async move {
                        let gql = ctx.data::<GqlContext>().map_err(|e| e.message)?;
                        let endpoint = res
                            .endpoints
                            .as_ref()
                            .and_then(|e| e.get("update"))
                            .cloned()
                            .ok_or("update endpoint missing")?;
                        rbac::enforce(endpoint.auth.as_ref(), gql.user.as_ref())
                            .map_err(|e| e.to_string())?;
                        let id_str = ctx
                            .args
                            .get("id")
                            .and_then(|v| v.string().ok())
                            .ok_or("id required")?;
                        let id = uuid::Uuid::parse_str(id_str).map_err(|e| e.to_string())?;
                        let store_opt = store_for_or_error(&gql.state, &res)?;
                        if rbac::needs_owner_check(endpoint.auth.as_ref(), gql.user.as_ref()) {
                            let existing = if let Some(store) = &store_opt {
                                store
                                    .find_by_id(&id)
                                    .await
                                    .map_err(|e: ShaperailError| e.to_string())?
                            } else {
                                let rq = ResourceQuery::new(&res, &gql.state.pool);
                                rq.find_by_id(&id)
                                    .await
                                    .map_err(|e: ShaperailError| e.to_string())?
                            };
                            if let Some(ref u) = gql.user {
                                rbac::check_owner(u, &existing.0).map_err(|e| e.to_string())?;
                            }
                        }
                        let input_accessor = ctx.args.try_get("input").map_err(|e| e.message)?;
                        let json_val = input_accessor
                            .as_value()
                            .clone()
                            .into_json()
                            .map_err(|e| e.to_string())?;
                        let input_data = extract_input_from_value(&json_val, &res, &endpoint)
                            .map_err(|e| e.to_string())?;
                        validate_input(&input_data, &res).map_err(|e| e.to_string())?;
                        let row = if let Some(store) = store_opt {
                            store
                                .update_by_id(&id, &input_data)
                                .await
                                .map_err(|e: ShaperailError| e.to_string())?
                        } else {
                            let rq = ResourceQuery::new(&res, &gql.state.pool);
                            rq.update_by_id(&id, &input_data)
                                .await
                                .map_err(|e: ShaperailError| e.to_string())?
                        };
                        run_write_side_effects(&gql.state, &res, &endpoint, "updated", &row.0)
                            .await;
                        Ok(Some(json_to_gql_value(&row.0)))
                    })
                },
            )
            .argument(InputValue::new("id", TypeRef::named("String")))
            .argument(InputValue::new(
                "input",
                TypeRef::named(input_type_name.clone()),
            ));
            mutation = mutation.field(update_field);
        }

        if resource
            .endpoints
            .as_ref()
            .map(|e| e.contains_key("delete"))
            .unwrap_or(false)
        {
            let res = resource.clone();
            let endpoint = resource
                .endpoints
                .as_ref()
                .and_then(|e| e.get("delete"))
                .cloned()
                .unwrap_or_else(|| EndpointSpec {
                    method: Some(HttpMethod::Delete),
                    path: Some(format!("/{}/:id", resource.resource)),
                    auth: None,
                    input: None,
                    filters: None,
                    search: None,
                    pagination: None,
                    sort: None,
                    cache: None,
                    controller: None,
                    events: None,
                    jobs: None,
                    upload: None,
                    soft_delete: true,
                });
            let delete_field = Field::new(
                format!("delete_{}", resource.resource),
                single_type,
                move |ctx| {
                    let res = res.clone();
                    let endpoint = endpoint.clone();
                    FieldFuture::new(async move {
                        let gql = ctx.data::<GqlContext>().map_err(|e| e.message)?;
                        rbac::enforce(endpoint.auth.as_ref(), gql.user.as_ref())
                            .map_err(|e| e.to_string())?;
                        let id_str = ctx
                            .args
                            .get("id")
                            .and_then(|v| v.string().ok())
                            .ok_or("id required")?;
                        let id = uuid::Uuid::parse_str(id_str).map_err(|e| e.to_string())?;
                        let store_opt = store_for_or_error(&gql.state, &res)?;
                        if rbac::needs_owner_check(endpoint.auth.as_ref(), gql.user.as_ref()) {
                            let existing = if let Some(store) = &store_opt {
                                store
                                    .find_by_id(&id)
                                    .await
                                    .map_err(|e: ShaperailError| e.to_string())?
                            } else {
                                let rq = ResourceQuery::new(&res, &gql.state.pool);
                                rq.find_by_id(&id)
                                    .await
                                    .map_err(|e: ShaperailError| e.to_string())?
                            };
                            if let Some(ref u) = gql.user {
                                rbac::check_owner(u, &existing.0).map_err(|e| e.to_string())?;
                            }
                        }
                        let (return_data, deleted_data) = if endpoint.soft_delete {
                            let row = if let Some(store) = store_opt {
                                store
                                    .soft_delete_by_id(&id)
                                    .await
                                    .map_err(|e: ShaperailError| e.to_string())?
                            } else {
                                let rq = ResourceQuery::new(&res, &gql.state.pool);
                                rq.soft_delete_by_id(&id)
                                    .await
                                    .map_err(|e: ShaperailError| e.to_string())?
                            };
                            let data = row.0.clone();
                            (data.clone(), data)
                        } else {
                            let row = if let Some(store) = store_opt {
                                store
                                    .hard_delete_by_id(&id)
                                    .await
                                    .map_err(|e: ShaperailError| e.to_string())?
                            } else {
                                let rq = ResourceQuery::new(&res, &gql.state.pool);
                                rq.hard_delete_by_id(&id)
                                    .await
                                    .map_err(|e: ShaperailError| e.to_string())?
                            };
                            let data = row.0.clone();
                            (data.clone(), data)
                        };
                        if !endpoint.soft_delete {
                            schedule_file_cleanup(&res, &deleted_data);
                        }
                        run_write_side_effects(
                            &gql.state,
                            &res,
                            &endpoint,
                            "deleted",
                            &deleted_data,
                        )
                        .await;
                        Ok(Some(json_to_gql_value(&return_data)))
                    })
                },
            )
            .argument(InputValue::new("id", TypeRef::named("String")));
            mutation = mutation.field(delete_field);
        }
    }

    mutation
}

/// Builds one Object type per resource with schema fields and relation fields.
fn build_resource_objects(resources: &[ResourceDefinition]) -> Vec<Object> {
    let mut objects = Vec::new();
    let resources_ref = resources;

    for resource in resources_ref {
        let type_name = object_type_name(&resource.resource);
        let mut obj = Object::new(type_name.clone());

        for (field_name, field_schema) in &resource.schema {
            let field_type =
                field_type_to_type_ref(&field_schema.field_type, field_schema.required);
            let name = field_name.clone();
            let field = Field::new(name.clone(), field_type, move |ctx| {
                let name = name.clone();
                FieldFuture::new(async move {
                    let parent = ctx.parent_value.try_to_value().map_err(|e| e.message)?;
                    let val = match parent {
                        Value::Object(map) => map
                            .iter()
                            .find(|(k, _)| k.as_str() == name.as_str())
                            .map(|(_, v)| v.clone())
                            .unwrap_or(Value::Null),
                        _ => Value::Null,
                    };
                    Ok(Some(val))
                })
            });
            obj = obj.field(field);
        }

        if let Some(relations) = &resource.relations {
            for (relation_name, relation) in relations {
                let related_type_name = object_type_name(&relation.resource);
                let field_ty = match relation.relation_type {
                    RelationType::HasMany => TypeRef::named_list(related_type_name.clone()),
                    RelationType::BelongsTo | RelationType::HasOne => {
                        TypeRef::named(related_type_name.clone())
                    }
                };
                let res = resource.clone();
                let rel_name = relation_name.clone();
                let rel = relation.clone();
                let field = Field::new(rel_name.clone(), field_ty, move |ctx| {
                    let res = res.clone();
                    let rel = rel.clone();
                    let rel_name = rel_name.clone();
                    FieldFuture::new(async move {
                        let gql = ctx.data::<GqlContext>().map_err(|e| e.message)?;
                        let parent = ctx.parent_value.try_to_value().map_err(|e| e.message)?;
                        let loader = &gql.loader;
                        match rel.relation_type {
                            RelationType::BelongsTo => {
                                let key = rel
                                    .key
                                    .clone()
                                    .unwrap_or_else(|| format!("{}_id", rel_name));
                                let fk_str = match &parent {
                                    Value::Object(map) => map
                                        .iter()
                                        .find(|(k, _)| k.as_str() == key.as_str())
                                        .and_then(|(_, v)| match v {
                                            Value::String(s) => Some(s.as_str()),
                                            _ => None,
                                        }),
                                    _ => None,
                                };
                                let Some(fk_str) = fk_str else {
                                    return Ok(Some(Value::Null));
                                };
                                let fk =
                                    uuid::Uuid::parse_str(fk_str).map_err(|e| e.to_string())?;
                                let row = loader
                                    .load_by_id(&rel.resource, &fk)
                                    .await
                                    .map_err(|e: ShaperailError| e.to_string())?;
                                match row {
                                    Some(r) => Ok(Some(json_to_gql_value(&r.0))),
                                    None => Ok(Some(Value::Null)),
                                }
                            }
                            RelationType::HasMany | RelationType::HasOne => {
                                let pk = res
                                    .schema
                                    .iter()
                                    .find(|(_, fs)| fs.primary)
                                    .map(|(n, _)| n.as_str())
                                    .unwrap_or("id");
                                let parent_id = match &parent {
                                    Value::Object(map) => map
                                        .iter()
                                        .find(|(k, _)| k.as_str() == pk)
                                        .and_then(|(_, v)| match v {
                                            Value::String(s) => Some(s.as_str()),
                                            _ => None,
                                        }),
                                    _ => None,
                                };
                                let Some(id_str) = parent_id else {
                                    return Ok(Some(Value::Null));
                                };
                                let fk = rel.foreign_key.as_deref().unwrap_or("id");
                                let rows = loader
                                    .load_by_filter(&rel.resource, fk, id_str)
                                    .await
                                    .map_err(|e: ShaperailError| e.to_string())?;
                                let list: Vec<Value> =
                                    rows.into_iter().map(|r| json_to_gql_value(&r.0)).collect();
                                if rel.relation_type == RelationType::HasOne {
                                    Ok(Some(list.into_iter().next().unwrap_or(Value::Null)))
                                } else {
                                    Ok(Some(Value::List(list)))
                                }
                            }
                        }
                    })
                });
                obj = obj.field(field);
            }
        }

        objects.push(obj);
    }

    objects
}

/// Builds the Subscription object from declared WebSocket events on resources.
///
/// For each resource, if endpoints declare `events`, a subscription field is created
/// for each event (e.g., `user_created`, `order_updated`). The subscription streams
/// events via a broadcast channel.
fn build_subscription_object(resources: &[ResourceDefinition]) -> Subscription {
    let mut subscription = Subscription::new("Subscription");

    for resource in resources {
        let type_name = object_type_name(&resource.resource);
        let endpoints = match &resource.endpoints {
            Some(eps) => eps,
            None => continue,
        };

        for (_ep_name, endpoint) in endpoints {
            let events = match &endpoint.events {
                Some(events) => events,
                None => continue,
            };

            for event_name in events {
                let event = event_name.clone();
                let sub_name = event.replace('.', "_");
                let field = SubscriptionField::new(
                    sub_name,
                    TypeRef::named(type_name.clone()),
                    move |ctx| {
                        let event = event.clone();
                        SubscriptionFieldFuture::new(async move {
                            let gql = ctx.data::<GqlContext>().map_err(|e| e.message)?;
                            // Create a broadcast receiver from the event emitter.
                            let rx = gql.state.event_bus_subscribe(&event);
                            let stream = futures_util::stream::unfold(rx, |mut rx| async move {
                                match rx.recv().await {
                                    Ok(payload) => {
                                        let val = json_to_gql_value(&payload);
                                        Some((Ok(val), rx))
                                    }
                                    Err(_) => None,
                                }
                            });
                            Ok(stream)
                        })
                    },
                );
                subscription = subscription.field(field);
            }
        }
    }

    subscription
}

/// Builds the full GraphQL schema from resources and app state.
///
/// If `gql_config` is provided, depth and complexity limits are taken from it.
/// Otherwise, defaults are used (depth: 16, complexity: 256).
pub fn build_schema(
    resources: &[ResourceDefinition],
    _state: Arc<AppState>,
) -> Result<GraphQLSchema, ShaperailError> {
    build_schema_with_config(resources, _state, None)
}

/// Builds the full GraphQL schema with configurable depth and complexity limits.
pub fn build_schema_with_config(
    resources: &[ResourceDefinition],
    _state: Arc<AppState>,
    gql_config: Option<&GraphQLConfig>,
) -> Result<GraphQLSchema, ShaperailError> {
    let depth_limit = gql_config.map(|c| c.depth_limit).unwrap_or(16);
    let complexity_limit = gql_config.map(|c| c.complexity_limit).unwrap_or(256);

    let query = build_query_object(resources);
    let mutation = build_mutation_object(resources);
    let subscription = build_subscription_object(resources);
    let resource_objects = build_resource_objects(resources);
    let input_objects = build_input_objects(resources);

    let mut builder: SchemaBuilder = Schema::build("Query", Some("Mutation"), Some("Subscription"))
        .register(query)
        .register(mutation)
        .register(subscription)
        .limit_depth(depth_limit)
        .limit_complexity(complexity_limit);

    for obj in input_objects {
        builder = builder.register(obj);
    }
    for obj in resource_objects {
        builder = builder.register(obj);
    }

    builder
        .finish()
        .map_err(|e| ShaperailError::Internal(format!("GraphQL schema build failed: {e}")))
}
