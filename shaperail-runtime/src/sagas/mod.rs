//! Saga orchestration: distributed multi-step transactions with compensating actions.

pub mod executor;

pub use executor::SagaExecutor;
