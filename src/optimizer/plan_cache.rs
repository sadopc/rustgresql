//! Query plan caching
//!
//! Provides caching of execution plans for repeated queries.

use crate::executor::planner::ExecutionPlan;
use std::collections::HashMap;

/// Cached plan information
#[derive(Debug, Clone)]
pub struct CachedPlan {
    pub plan: ExecutionPlan,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub usage_count: u64,
    pub last_used: chrono::DateTime<chrono::Utc>,
    pub estimated_cost: f64,
    pub hit_ratio: f64,  // Cache hit ratio for this plan type
}

impl CachedPlan {
    pub fn new(plan: ExecutionPlan, estimated_cost: f64) -> Self {
        let now = chrono::Utc::now();
        Self {
            plan,
            created_at: now,
            usage_count: 0,
            last_used: now,
            estimated_cost,
            hit_ratio: 0.0,
        }
    }

    pub fn mark_used(&mut self) {
        self.usage_count += 1;
        self.last_used = chrono::Utc::now();

        // Update hit ratio (simplified)
        self.hit_ratio = (self.usage_count as f64) /
                        ((chrono::Utc::now() - self.created_at).num_seconds().max(1) as f64);
    }

    pub fn is_expired(&self, max_age_secs: i64) -> bool {
        let age = chrono::Utc::now() - self.created_at;
        age.num_seconds() > max_age_secs
    }

    pub fn is_stale(&self, max_unused_secs: i64) -> bool {
        let unused_time = chrono::Utc::now() - self.last_used;
        unused_time.num_seconds() > max_unused_secs
    }
}

/// Plan cache configuration
#[derive(Debug, Clone)]
pub struct PlanCacheConfig {
    pub max_size: usize,
    pub max_age_secs: i64,        // Maximum age before expiration
    pub max_unused_secs: i64,     // Maximum time unused before considered stale
    pub cleanup_interval_secs: u64, // How often to run cleanup
    pub enable_prepared_statements: bool, // Whether to support prepared statement caching
}

impl Default for PlanCacheConfig {
    fn default() -> Self {
        Self {
            max_size: 1000,
            max_age_secs: 3600,      // 1 hour
            max_unused_secs: 1800,   // 30 minutes
            cleanup_interval_secs: 300, // 5 minutes
            enable_prepared_statements: true,
        }
    }
}

/// Plan cache for storing and retrieving execution plans
#[derive(Debug)]
pub struct PlanCache {
    cache: HashMap<String, CachedPlan>,
    config: PlanCacheConfig,
    stats: PlanCacheStats,
    last_cleanup: chrono::DateTime<chrono::Utc>,
}

impl PlanCache {
    /// Create new plan cache
    pub fn new() -> Self {
        Self::with_config(PlanCacheConfig::default())
    }

    /// Create plan cache with specific configuration
    pub fn with_config(config: PlanCacheConfig) -> Self {
        Self {
            cache: HashMap::new(),
            stats: PlanCacheStats {
                size: 0,
                max_size: config.max_size,
                total_usage: 0,
                hits: 0,
                misses: 0,
                evictions: 0,
                expirations: 0,
            },
            config,
            last_cleanup: chrono::Utc::now(),
        }
    }

    /// Get cached plan for query
    pub fn get(&mut self, query_key: &str) -> Option<&mut CachedPlan> {
        // Run cleanup if needed
        self.maybe_cleanup();

        if let Some(cached_plan) = self.cache.get_mut(query_key) {
            cached_plan.mark_used();
            self.stats.hits += 1;
            self.stats.total_usage += 1;
            Some(cached_plan)
        } else {
            self.stats.misses += 1;
            None
        }
    }

    /// Store plan in cache with estimated cost
    pub fn put(&mut self, query_key: String, plan: ExecutionPlan, estimated_cost: f64) -> Option<ExecutionPlan> {
        // Run cleanup if needed
        self.maybe_cleanup();

        if self.cache.len() >= self.config.max_size {
            if !self.evict_lru() {
                // Couldn't evict any eligible plans
                return None;
            }
        }

        let cached_plan = CachedPlan::new(plan, estimated_cost);
        let old_plan = self.cache.insert(query_key.clone(), cached_plan).map(|old| old.plan);

        if old_plan.is_some() {
            // Replaced existing plan
            self.stats.size = self.cache.len();
        } else {
            // Added new plan
            self.stats.size = self.cache.len();
        }

        old_plan
    }

    /// Remove plan from cache
    pub fn remove(&mut self, query_key: &str) -> Option<ExecutionPlan> {
        self.cache.remove(query_key).map(|old| {
            self.stats.size = self.cache.len();
            old.plan
        })
    }

    /// Clear all cached plans
    pub fn clear(&mut self) {
        self.cache.clear();
        self.stats.size = 0;
    }

    /// Get cache statistics
    pub fn stats(&self) -> &PlanCacheStats {
        &self.stats
    }

    /// Run cleanup if needed
    fn maybe_cleanup(&mut self) {
        let now = chrono::Utc::now();
        let secs_since_cleanup = (now - self.last_cleanup).num_seconds() as u64;

        if secs_since_cleanup >= self.config.cleanup_interval_secs {
            self.cleanup_expired_and_stale();
            self.last_cleanup = now;
        }
    }

    /// Cleanup expired and stale plans
    fn cleanup_expired_and_stale(&mut self) {
        let mut keys_to_remove = Vec::new();

        for (key, plan) in &self.cache {
            if plan.is_expired(self.config.max_age_secs) ||
               plan.is_stale(self.config.max_unused_secs) {
                keys_to_remove.push(key.clone());
                self.stats.expirations += 1;
            }
        }

        for key in keys_to_remove {
            self.cache.remove(&key);
        }

        self.stats.size = self.cache.len();
    }

    /// Evict least recently used plan
    fn evict_lru(&mut self) -> bool {
        if let Some((lru_key, _)) = self.cache
            .iter()
            .min_by_key(|(_, cached)| cached.last_used)
            .map(|(key, cached)| (key.clone(), cached.clone())) {
            self.cache.remove(&lru_key);
            self.stats.evictions += 1;
            self.stats.size = self.cache.len();
            true
        } else {
            false
        }
    }

    /// Get configuration reference
    pub fn config(&self) -> &PlanCacheConfig {
        &self.config
    }

    /// Update configuration
    pub fn update_config<F>(&mut self, updater: F)
    where
        F: FnOnce(&mut PlanCacheConfig),
    {
        updater(&mut self.config);
        self.stats.max_size = self.config.max_size;

        // If new max_size is smaller, evict excess entries
        while self.cache.len() > self.config.max_size {
            if !self.evict_lru() {
                break;
            }
        }
    }

    /// Generate query key from SQL text (simplified - in production would use normalized form)
    pub fn normalize_query(&self, query: &str) -> String {
        // Very basic normalization - just trim and convert to lowercase
        // In a real implementation, this would:
        // - Remove extra whitespace
        // - Normalize identifiers
        // - Parameterize literals
        // - Handle different SQL dialects
        query.trim().to_lowercase()
    }

    /// Get hit ratio (0.0 to 1.0)
    pub fn hit_ratio(&self) -> f64 {
        let total_requests = self.stats.hits + self.stats.misses;
        if total_requests == 0 {
            0.0
        } else {
            self.stats.hits as f64 / total_requests as f64
        }
    }

    /// Get memory usage estimate (rough)
    pub fn estimated_memory_usage(&self) -> usize {
        // Rough estimate: each cached plan + metadata ~ 1KB
        self.cache.len() * 1024
    }
}

/// Plan cache statistics
#[derive(Debug)]
pub struct PlanCacheStats {
    pub size: usize,
    pub max_size: usize,
    pub total_usage: u64,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub expirations: u64,
}

impl PlanCacheStats {
    pub fn new(max_size: usize) -> Self {
        Self {
            size: 0,
            max_size,
            total_usage: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
            expirations: 0,
        }
    }

    /// Reset all statistics
    pub fn reset(&mut self) {
        self.size = 0;
        self.total_usage = 0;
        self.hits = 0;
        self.misses = 0;
        self.evictions = 0;
        self.expirations = 0;
    }

    /// Get hit ratio (0.0 to 1.0)
    pub fn hit_ratio(&self) -> f64 {
        let total_requests = self.hits + self.misses;
        if total_requests == 0 {
            0.0
        } else {
            self.hits as f64 / total_requests as f64
        }
    }

    /// Check if cache is at capacity
    pub fn is_at_capacity(&self) -> bool {
        self.size >= self.max_size
    }

    /// Get cache utilization percentage (0.0 to 1.0)
    pub fn utilization(&self) -> f64 {
        if self.max_size == 0 {
            0.0
        } else {
            self.size as f64 / self.max_size as f64
        }
    }
}