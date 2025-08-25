// Stateless components, used by agents.
pub mod canonicalizer;
pub mod extractor;
pub mod fetcher;
pub mod policy;

// Queue for task coordination.
pub mod frontier;

// Registry of URLs, shared by all agents.
pub mod registry;
