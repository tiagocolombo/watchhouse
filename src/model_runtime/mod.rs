//! Versioned model-client transport. Proposal data never grants workflow or external-effect authority.
pub mod cgroup;
pub mod codex;

pub mod client;
pub mod policy;

pub mod protocol;

pub mod fixture;

pub mod ops;

#[cfg(test)]
mod tests;

pub mod adoption;

pub mod claude;
