//! GraphQL HTTP handler and playground (M15).

use std::sync::Arc;

use actix_web::{web, HttpRequest, HttpResponse};
use async_graphql_actix_web::{GraphQLRequest, GraphQLResponse};

use super::dataloader::RelationLoader;
use super::schema::{GqlContext, GraphQLSchema};
use crate::auth::extractor::try_extract_auth;
use crate::handlers::crud::AppState;

/// POST /graphql handler. Expects schema and state in app_data; injects GqlContext
/// (with auth user and DataLoader) into the request.
pub async fn graphql_handler(
    req: HttpRequest,
    schema: web::Data<GraphQLSchema>,
    state: web::Data<Arc<AppState>>,
    request: GraphQLRequest,
) -> GraphQLResponse {
    let user = try_extract_auth(&req);
    let loader = RelationLoader::new(state.get_ref().clone(), state.resources.clone());
    let context = GqlContext {
        state: state.get_ref().clone(),
        resources: state.resources.clone(),
        user,
        loader,
    };
    let mut gql_req = request.into_inner();
    gql_req = gql_req.data(context);
    let response = schema.execute(gql_req).await;
    response.into()
}

/// GET /graphql/playground — serves a minimal GraphQL Playground HTML page.
pub async fn playground_handler() -> HttpResponse {
    let html = r#"<!DOCTYPE html>
<html>
<head><title>GraphQL Playground</title></head>
<body>
  <h1>GraphQL Playground</h1>
  <p>Endpoint: <code>POST /graphql</code></p>
  <textarea id="query" rows="8" style="width:90%;display:block;margin:1em 0;">query { __typename }</textarea>
  <button onclick="run()">Run</button>
  <pre id="result"></pre>
  <script>
    async function run() {
      const query = document.getElementById('query').value;
      const res = await fetch('/graphql', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ query }) });
      const data = await res.json();
      document.getElementById('result').textContent = JSON.stringify(data, null, 2);
    }
  </script>
</body>
</html>"#;
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html)
}
