//! Middleware for authentication and rate limiting

pub mod auth;
pub mod rate_limit;

pub use auth::{authenticate, token_exchange, Claims, SESSION_COOKIE};
pub use rate_limit::{rate_limit_middleware, RateLimiter};
