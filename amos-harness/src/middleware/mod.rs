//! Middleware for authentication, rate limiting, and security headers

pub mod admin;
pub mod auth;
pub mod rate_limit;
pub mod security_headers;

pub use admin::AdminAuth;
pub use auth::{authenticate, token_exchange, Claims, SESSION_COOKIE};
pub use rate_limit::{rate_limit_middleware, RateLimiter};
pub use security_headers::security_headers;
