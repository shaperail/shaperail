//! GraphQL support (M15). Dynamic schema from resources, query/mutation resolvers,
//! DataLoader for N+1 prevention, subscriptions, optional playground.

mod dataloader;
mod handler;
mod schema;

pub use dataloader::RelationLoader;
pub use handler::{graphql_handler, playground_handler};
pub use schema::{build_schema, build_schema_with_config, GqlContext, GraphQLSchema};
