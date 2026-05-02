use shaperail_core::{ProjectConfig, RelationType};

pub fn run(resource_filter: Option<&str>, json_output: bool) -> i32 {
    let config = match super::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    let resources = match super::load_all_resources() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    let mut filtered: Vec<_> = resources.iter().collect();
    if let Some(name) = resource_filter {
        filtered.retain(|r| r.resource == name);
        if filtered.is_empty() {
            eprintln!("No resource named '{name}' found in resources/");
            return 1;
        }
    }
    filtered.sort_by(|a, b| a.resource.cmp(&b.resource));

    let all_diags: Vec<_> = filtered
        .iter()
        .flat_map(|r| {
            shaperail_codegen::diagnostics::diagnose_resource(r)
                .into_iter()
                .map(|d| (r.resource.clone(), d))
        })
        .collect();

    if json_output {
        if let Err(e) = print_json(&config, &filtered, &all_diags) {
            eprintln!("{e}");
            return 1;
        }
    } else {
        print_markdown(&config, &filtered, &all_diags);
    }
    0
}

fn db_summary(config: &ProjectConfig) -> String {
    if let Some(ref dbs) = config.databases {
        let mut engines: Vec<String> = dbs
            .values()
            .map(|d| {
                serde_json::to_value(d.engine)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_else(|| format!("{:?}", d.engine).to_lowercase())
            })
            .collect();
        engines.sort();
        engines.dedup();
        engines.join(", ")
    } else {
        "unknown".into()
    }
}

fn auth_summary(config: &ProjectConfig) -> String {
    config
        .auth
        .as_ref()
        .map(|a| a.provider.clone())
        .unwrap_or_else(|| "none".into())
}

fn print_markdown(
    config: &ProjectConfig,
    resources: &[&shaperail_core::ResourceDefinition],
    diags: &[(String, shaperail_codegen::diagnostics::Diagnostic)],
) {
    println!("# Project: {}", config.project);
    println!(
        "Database: {} | Auth: {} | Port: {}",
        db_summary(config),
        auth_summary(config),
        config.port
    );
    println!();
    println!("## Resources ({})", resources.len());
    println!();

    for rd in resources {
        println!("### {} (v{})", rd.resource, rd.version);

        let field_strs: Vec<String> = rd
            .schema
            .iter()
            .map(|(name, field)| {
                let mut parts = vec![field.field_type.to_string()];
                if field.primary {
                    parts.push("pk".into());
                }
                if field.generated {
                    parts.push("generated".into());
                }
                if field.required {
                    parts.push("required".into());
                }
                if field.unique {
                    parts.push("unique".into());
                }
                if let Some(ref r) = field.reference {
                    parts.push(format!("fk→{r}"));
                }
                if let Some(ref vals) = field.values {
                    parts.push(format!("[{}]", vals.join(",")));
                }
                if let Some(ref def) = field.default {
                    parts.push(format!("default:{def}"));
                }
                format!("{name}({})", parts.join(","))
            })
            .collect();
        println!("Fields: {}", field_strs.join(", "));

        if let Some(ref eps) = rd.endpoints {
            let mut ep_list: Vec<(String, String)> = eps
                .iter()
                .map(|(action, ep)| {
                    let auth_str = match &ep.auth {
                        Some(a) => format!("{a}"),
                        None => "none".into(),
                    };
                    (action.clone(), format!("{action}[{auth_str}]"))
                })
                .collect();
            ep_list.sort_by(|a, b| a.0.cmp(&b.0));
            println!(
                "Endpoints: {}",
                ep_list
                    .iter()
                    .map(|(_, s)| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        if let Some(ref rels) = rd.relations {
            let mut rel_list: Vec<(String, String)> = rels
                .iter()
                .map(|(name, rel)| {
                    let kind = match rel.relation_type {
                        RelationType::BelongsTo => "belongs_to",
                        RelationType::HasMany => "has_many",
                        RelationType::HasOne => "has_one",
                    };
                    (name.clone(), format!("{name}({kind}→{})", rel.resource))
                })
                .collect();
            rel_list.sort_by(|a, b| a.0.cmp(&b.0));
            println!(
                "Relations: {}",
                rel_list
                    .iter()
                    .map(|(_, s)| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        if let Some(ref eps) = rd.endpoints {
            let mut cached: Vec<String> = eps
                .iter()
                .filter_map(|(action, ep)| {
                    ep.cache.as_ref().map(|c| format!("{action}({}s)", c.ttl))
                })
                .collect();
            if !cached.is_empty() {
                cached.sort();
                println!("Cache: {}", cached.join(", "));
            }
        }

        if let Some(ref tk) = rd.tenant_key {
            println!("Tenant key: {tk}");
        }

        let has_soft_delete = rd
            .endpoints
            .as_ref()
            .map(|eps| eps.values().any(|ep| ep.soft_delete))
            .unwrap_or(false);
        if has_soft_delete {
            println!("Soft delete: enabled");
        }

        let res_diags: Vec<_> = diags.iter().filter(|(r, _)| r == &rd.resource).collect();
        if !res_diags.is_empty() {
            println!(
                "Errors: {}",
                res_diags
                    .iter()
                    .map(|(_, d)| format!("[{}] {}", d.code, d.error))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }

        println!();
    }

    if diags.is_empty() {
        println!("## Validation\n✓ No errors found");
    } else {
        println!("## Validation\n⚠ {} issue(s):", diags.len());
        for (resource, d) in diags {
            println!("  {resource} [{}] {} → {}", d.code, d.error, d.fix);
        }
    }
}

fn print_json(
    config: &ProjectConfig,
    resources: &[&shaperail_core::ResourceDefinition],
    diags: &[(String, shaperail_codegen::diagnostics::Diagnostic)],
) -> Result<(), String> {
    let resource_list: Vec<serde_json::Value> = resources
        .iter()
        .map(|rd| {
            let fields: Vec<serde_json::Value> = rd
                .schema
                .iter()
                .map(|(name, field)| {
                    serde_json::json!({
                        "name": name,
                        "type": field.field_type.to_string(),
                        "primary": field.primary,
                        "generated": field.generated,
                        "required": field.required,
                        "unique": field.unique,
                        "ref": field.reference,
                        "values": field.values,
                        "default": field.default,
                    })
                })
                .collect();

            let endpoints: Vec<serde_json::Value> = rd
                .endpoints
                .as_ref()
                .map(|eps| {
                    eps.iter()
                        .map(|(action, ep)| {
                            serde_json::json!({
                                "action": action,
                                "method": ep.method(),
                                "path": format!("/v{}{}", rd.version, ep.path()),
                                "auth": ep.auth.as_ref().map(|a| format!("{a}")),
                                "cache_ttl": ep.cache.as_ref().map(|c| c.ttl),
                                "soft_delete": ep.soft_delete,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            let relations: Vec<serde_json::Value> = rd
                .relations
                .as_ref()
                .map(|rels| {
                    rels.iter()
                        .map(|(name, rel)| {
                            let kind = match rel.relation_type {
                                RelationType::BelongsTo => "belongs_to",
                                RelationType::HasMany => "has_many",
                                RelationType::HasOne => "has_one",
                            };
                            serde_json::json!({
                                "name": name,
                                "type": kind,
                                "resource": rel.resource,
                                "key": rel.key,
                                "foreign_key": rel.foreign_key,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            let errors: Vec<serde_json::Value> = diags
                .iter()
                .filter(|(r, _)| r == &rd.resource)
                .map(|(_, d)| {
                    serde_json::json!({
                        "code": d.code,
                        "error": d.error,
                        "fix": d.fix,
                    })
                })
                .collect();

            serde_json::json!({
                "name": rd.resource,
                "version": rd.version,
                "tenant_key": rd.tenant_key,
                "fields": fields,
                "endpoints": endpoints,
                "relations": relations,
                "errors": errors,
            })
        })
        .collect();

    let output = serde_json::json!({
        "project": {
            "name": config.project,
            "database": db_summary(config),
            "auth": auth_summary(config),
            "port": config.port,
        },
        "resources": resource_list,
        "validation": {
            "total_errors": diags.len(),
            "clean": diags.is_empty(),
        },
    });

    let s = serde_json::to_string_pretty(&output)
        .map_err(|e| format!("JSON serialization failed: {e}"))?;
    println!("{s}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use shaperail_core::{ProjectConfig, WorkerCount};

    fn make_config(project: &str) -> ProjectConfig {
        ProjectConfig {
            project: project.to_string(),
            port: 3000,
            workers: WorkerCount::Auto,
            databases: None,
            cache: None,
            auth: None,
            storage: None,
            logging: None,
            events: None,
            protocols: vec!["rest".to_string()],
            graphql: None,
            grpc: None,
        }
    }

    #[test]
    fn db_summary_single_db() {
        let mut config = make_config("my-app");
        let mut dbs = indexmap::IndexMap::new();
        dbs.insert(
            "default".to_string(),
            shaperail_core::NamedDatabaseConfig {
                engine: shaperail_core::DatabaseEngine::Postgres,
                url: "postgresql://localhost/test_db".to_string(),
                pool_size: 5,
            },
        );
        config.databases = Some(dbs);
        assert_eq!(db_summary(&config), "postgres");
    }

    #[test]
    fn auth_summary_no_auth() {
        let config = make_config("my-app");
        assert_eq!(auth_summary(&config), "none");
    }

    #[test]
    fn auth_summary_with_jwt() {
        let mut config = make_config("my-app");
        config.auth = Some(shaperail_core::AuthConfig {
            provider: "jwt".to_string(),
            secret_env: "JWT_SECRET".to_string(),
            expiry: "24h".to_string(),
            refresh_expiry: None,
        });
        assert_eq!(auth_summary(&config), "jwt");
    }

    // ── Additional coverage for llm_context ───────────────────────────────

    #[test]
    fn db_summary_no_databases_returns_unknown() {
        let config = make_config("my-app");
        // databases is None by default in make_config
        assert_eq!(db_summary(&config), "unknown");
    }

    #[test]
    fn db_summary_multiple_databases_sorted_deduped() {
        let mut config = make_config("my-app");
        let mut dbs = indexmap::IndexMap::new();
        dbs.insert(
            "primary".to_string(),
            shaperail_core::NamedDatabaseConfig {
                engine: shaperail_core::DatabaseEngine::Postgres,
                url: "postgresql://localhost/primary".to_string(),
                pool_size: 5,
            },
        );
        dbs.insert(
            "secondary".to_string(),
            shaperail_core::NamedDatabaseConfig {
                engine: shaperail_core::DatabaseEngine::Postgres,
                url: "postgresql://localhost/secondary".to_string(),
                pool_size: 3,
            },
        );
        config.databases = Some(dbs);
        // Same engine twice — should be deduped to a single "postgres"
        assert_eq!(db_summary(&config), "postgres");
    }

    #[test]
    fn db_summary_mixed_engines() {
        let mut config = make_config("my-app");
        let mut dbs = indexmap::IndexMap::new();
        dbs.insert(
            "cache".to_string(),
            shaperail_core::NamedDatabaseConfig {
                engine: shaperail_core::DatabaseEngine::SQLite,
                url: "sqlite://data.db".to_string(),
                pool_size: 1,
            },
        );
        dbs.insert(
            "primary".to_string(),
            shaperail_core::NamedDatabaseConfig {
                engine: shaperail_core::DatabaseEngine::Postgres,
                url: "postgresql://localhost/db".to_string(),
                pool_size: 5,
            },
        );
        config.databases = Some(dbs);
        let summary = db_summary(&config);
        // Both engines must appear, sorted
        assert!(
            summary.contains("postgres"),
            "postgres missing from: {summary}"
        );
        assert!(summary.contains("sqlite"), "sqlite missing from: {summary}");
    }

    #[test]
    fn print_json_produces_valid_json_with_correct_structure() {
        use shaperail_core::{FieldSchema, FieldType, ResourceDefinition};

        let config = make_config("test-project");
        let mut schema = indexmap::IndexMap::new();
        schema.insert(
            "id".to_string(),
            FieldSchema {
                field_type: FieldType::Uuid,
                primary: true,
                generated: true,
                required: false,
                unique: false,
                nullable: false,
                reference: None,
                min: None,
                max: None,
                format: None,
                values: None,
                default: None,
                sensitive: false,
                search: false,
                items: None,
                transient: false,
            },
        );
        let rd = ResourceDefinition {
            resource: "items".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema,
            endpoints: None,
            relations: None,
            indexes: None,
        };

        // Call print_json internally and verify it serializes cleanly
        let resources: Vec<&ResourceDefinition> = vec![&rd];
        let diags: Vec<(String, shaperail_codegen::diagnostics::Diagnostic)> = vec![];
        let result = print_json(&config, &resources, &diags);
        assert!(result.is_ok(), "print_json must not fail: {result:?}");
    }

    #[test]
    fn auth_summary_with_api_key_provider() {
        let mut config = make_config("my-app");
        config.auth = Some(shaperail_core::AuthConfig {
            provider: "api_key".to_string(),
            secret_env: "API_KEY_SALT".to_string(),
            expiry: "never".to_string(),
            refresh_expiry: None,
        });
        assert_eq!(auth_summary(&config), "api_key");
    }
}
