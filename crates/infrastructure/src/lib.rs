//! Driven (outbound) adapters: the implementations of the ports that
//! `application` declared.
//!
//! This is where a technology choice is allowed to show — sqlx, reqwest, a
//! filesystem. Swap this crate out and the domain and use cases do not move.
