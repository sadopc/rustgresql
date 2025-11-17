//! Resource management for parallel query execution
//!
//! Provides coordinated management of memory, CPU, and I/O resources
//! across parallel workers to ensure optimal resource utilization.

use crate::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Condvar};
use std::thread;
use std::time::{Duration, Instant};

/// Types of resources that can be managed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceType {
    Memory,
    Cpu,
    Io,
    Network,
    Locks,
}

/// Resource constraints for parallel execution
#[derive(Debug, Clone)]
pub struct ResourceConstraints {
    /// Maximum memory per worker (bytes)
    pub max_memory_per_worker: usize,
    /// Maximum total memory across all workers (bytes)
    pub max_total_memory: usize,
    /// Maximum concurrent workers
    pub max_concurrent_workers: usize,
    /// Maximum concurrent I/O operations
    pub max_concurrent_io: usize,
    /// CPU affinity settings (optional)
    pub cpu_affinity: Option<Vec<usize>>,
    /// Timeout for resource acquisition (milliseconds)
    pub resource_timeout_ms: u64,
}

impl Default for ResourceConstraints {
    fn default() -> Self {
        // Reasonable defaults for a typical system
        let total_memory = if cfg!(target_os = "linux") {
            // Try to get system memory on Linux
            match std::fs::read_to_string("/proc/meminfo") {
                Ok(content) => {
                    if let Some(line) = content.lines().find(|l| l.starts_with("MemTotal:")) {
                        if let Some(kb_str) = line.split_whitespace().nth(1) {
                            if let Ok(kb) = kb_str.parse::<u64>() {
                                // Use 80% of system memory, leave 20% for OS
                                (kb as usize * 1024 * 8) / 10
                            } else {
                                8 * 1024 * 1024 * 1024 // 8GB fallback
                            }
                        } else {
                            8 * 1024 * 1024 * 1024
                        }
                    } else {
                        8 * 1024 * 1024 * 1024
                    }
                }
                Err(_) => 8 * 1024 * 1024 * 1024, // 8GB fallback
            }
        } else {
            8 * 1024 * 1024 * 1024 // 8GB for other OSes
        };

        Self {
            max_memory_per_worker: total_memory / 8, // 1/8 of total memory per worker
            max_total_memory: total_memory * 8 / 10, // Use 80% of total memory
            max_concurrent_workers: num_cpus::get(),
            max_concurrent_io: 64,
            cpu_affinity: None,
            resource_timeout_ms: 30000, // 30 seconds
        }
    }
}

/// Resource allocation request
#[derive(Debug)]
pub struct ResourceRequest {
    /// Requester identifier
    pub requester_id: u64,
    /// Type of resource
    pub resource_type: ResourceType,
    /// Amount of resource requested
    pub amount: usize,
    /// Priority (lower = higher priority)
    pub priority: u32,
    /// Timeout for acquisition
    pub timeout_ms: u64,
    /// Request timestamp
    pub requested_at: Instant,
}

/// Resource allocation grant
#[derive(Debug)]
pub struct ResourceGrant {
    /// Grant identifier
    pub grant_id: u64,
    /// Requester identifier
    pub requester_id: u64,
    /// Resource type
    pub resource_type: ResourceType,
    /// Granted amount
    pub amount: usize,
    /// Grant timestamp
    pub granted_at: Instant,
    /// Expiration time (None = no expiration)
    pub expires_at: Option<Instant>,
}

/// Resource usage tracking
#[derive(Debug, Clone)]
pub struct ResourceUsage {
    /// Total allocated amount
    pub allocated: usize,
    /// Currently in use amount
    pub in_use: usize,
    /// Peak usage
    pub peak_usage: usize,
    /// Number of active allocations
    pub active_allocations: usize,
    /// Total allocation count
    pub total_allocations: u64,
    /// Allocation failures
    pub allocation_failures: u64,
}

/// Memory tracker for individual allocations
#[derive(Debug)]
pub struct MemoryTracker {
    /// Current memory usage
    current_usage: usize,
    /// Peak memory usage
    peak_usage: usize,
    /// Allocation tracking
    allocations: HashMap<usize, usize>, // allocation_id -> size
    /// Next allocation ID
    next_allocation_id: u64,
}

impl MemoryTracker {
    fn new() -> Self {
        Self {
            current_usage: 0,
            peak_usage: 0,
            allocations: HashMap::new(),
            next_allocation_id: 1,
        }
    }

    fn allocate(&mut self, size: usize) -> Option<u64> {
        let allocation_id = self.next_allocation_id;
        self.next_allocation_id = allocation_id.wrapping_add(1);

        self.allocations.insert(allocation_id, size);
        self.current_usage += size;
        self.peak_usage = self.peak_usage.max(self.current_usage);

        Some(allocation_id)
    }

    fn deallocate(&mut self, allocation_id: u64) -> bool {
        if let Some(size) = self.allocations.remove(&allocation_id) {
            self.current_usage = self.current_usage.saturating_sub(size);
            true
        } else {
            false
        }
    }

    fn get_usage(&self) -> &usize {
        &self.current_usage
    }

    fn get_peak_usage(&self) -> usize {
        self.peak_usage
    }
}

/// CPU tracker for CPU utilization
#[derive(Debug)]
pub struct CpuTracker {
    /// Active CPU cores in use
    active_cores: usize,
    /// CPU utilization percentage (0.0 to 1.0)
    utilization: f64,
    /// Last update timestamp
    last_update: Instant,
}

impl CpuTracker {
    fn new() -> Self {
        Self {
            active_cores: 0,
            utilization: 0.0,
            last_update: Instant::now(),
        }
    }

    fn allocate_cores(&mut self, cores: usize) -> bool {
        // Simple core allocation - in practice this would interface with OS scheduling
        self.active_cores += cores;
        true
    }

    fn release_cores(&mut self, cores: usize) {
        self.active_cores = self.active_cores.saturating_sub(cores);
    }

    fn update_utilization(&mut self) {
        let now = Instant::now();
        // In practice, this would read actual CPU metrics from the system
        self.last_update = now;
        self.utilization = self.active_cores as f64 / num_cpus::get() as f64;
    }

    fn get_utilization(&self) -> f64 {
        self.utilization
    }

    fn get_active_cores(&self) -> usize {
        self.active_cores
    }
}

/// I/O tracker for concurrent I/O operations
#[derive(Debug)]
pub struct IoTracker {
    /// Active I/O operations
    active_operations: usize,
    /// Peak concurrent operations
    peak_operations: usize,
    /// Total operations completed
    total_operations: u64,
    /// Operations queue depth
    queue_depth: usize,
}

impl IoTracker {
    fn new() -> Self {
        Self {
            active_operations: 0,
            peak_operations: 0,
            total_operations: 0,
            queue_depth: 0,
        }
    }

    fn start_operation(&mut self) -> bool {
        self.active_operations += 1;
        self.peak_operations = self.peak_operations.max(self.active_operations);
        true
    }

    fn complete_operation(&mut self) {
        self.active_operations = self.active_operations.saturating_sub(1);
        self.total_operations += 1;
    }

    fn get_active_operations(&self) -> usize {
        self.active_operations
    }

    fn get_peak_operations(&self) -> usize {
        self.peak_operations
    }
}

/// Resource manager for coordinating parallel execution resources
pub struct ResourceManager {
    /// Resource constraints
    constraints: ResourceConstraints,
    /// Memory tracker
    memory_tracker: Arc<Mutex<MemoryTracker>>,
    /// CPU tracker
    cpu_tracker: Arc<Mutex<CpuTracker>>,
    /// I/O tracker
    io_tracker: Arc<Mutex<IoTracker>>,
    /// Resource usage tracking
    usage: Arc<Mutex<HashMap<ResourceType, ResourceUsage>>>,
    /// Pending resource requests
    pending_requests: Arc<Mutex<Vec<ResourceRequest>>>,
    /// Active resource grants
    active_grants: Arc<Mutex<HashMap<u64, ResourceGrant>>>,
    /// Next grant ID
    next_grant_id: Arc<Mutex<u64>>,
    /// Resource condition variable for notifications
    resource_cv: Arc<Condvar>,
    /// Background cleanup thread handle
    cleanup_handle: Option<thread::JoinHandle<()>>,
    /// Shutdown flag
    shutdown: Arc<Mutex<bool>>,
}

impl ResourceManager {
    /// Create a new resource manager
    pub fn new(constraints: ResourceConstraints) -> Result<Self> {
        let memory_tracker = Arc::new(Mutex::new(MemoryTracker::new()));
        let cpu_tracker = Arc::new(Mutex::new(CpuTracker::new()));
        let io_tracker = Arc::new(Mutex::new(IoTracker::new()));
        let usage = Arc::new(Mutex::new(HashMap::new()));
        let pending_requests = Arc::new(Mutex::new(Vec::new()));
        let active_grants = Arc::new(Mutex::new(HashMap::new()));
        let next_grant_id = Arc::new(Mutex::new(1));
        let resource_cv = Arc::new(Condvar::new());
        let shutdown = Arc::new(Mutex::new(false));

        // Initialize usage tracking
        {
            let mut usage_guard = usage.lock().unwrap();
            usage_guard.insert(ResourceType::Memory, ResourceUsage {
                allocated: 0,
                in_use: 0,
                peak_usage: 0,
                active_allocations: 0,
                total_allocations: 0,
                allocation_failures: 0,
            });
            usage_guard.insert(ResourceType::Cpu, ResourceUsage {
                allocated: 0,
                in_use: 0,
                peak_usage: 0,
                active_allocations: 0,
                total_allocations: 0,
                allocation_failures: 0,
            });
            usage_guard.insert(ResourceType::Io, ResourceUsage {
                allocated: 0,
                in_use: 0,
                peak_usage: 0,
                active_allocations: 0,
                total_allocations: 0,
                allocation_failures: 0,
            });
        }

        // Start cleanup thread
        let memory_tracker_clone = Arc::clone(&memory_tracker);
        let usage_clone = Arc::clone(&usage);
        let shutdown_clone = Arc::clone(&shutdown);
        let cleanup_handle = thread::Builder::new()
            .name("rustgresql-resource-cleanup".to_string())
            .spawn(move || {
                Self::cleanup_thread(memory_tracker_clone, usage_clone, shutdown_clone);
            })?;

        Ok(Self {
            constraints,
            memory_tracker,
            cpu_tracker,
            io_tracker,
            usage,
            pending_requests,
            active_grants,
            next_grant_id,
            resource_cv,
            cleanup_handle: Some(cleanup_handle),
            shutdown,
        })
    }

    /// Allocate memory for a worker
    pub fn allocate_memory(&self, requester_id: u64, amount: usize) -> Result<u64> {
        self.allocate_resource(ResourceRequest {
            requester_id,
            resource_type: ResourceType::Memory,
            amount,
            priority: 0,
            timeout_ms: self.constraints.resource_timeout_ms,
            requested_at: Instant::now(),
        }).map(|grant| grant.grant_id)
    }

    /// Deallocate memory
    pub fn deallocate_memory(&self, grant_id: u64) -> Result<()> {
        self.deallocate_resource(grant_id)
    }

    /// Allocate CPU cores for a worker
    pub fn allocate_cpu(&self, requester_id: u64, cores: usize) -> Result<u64> {
        self.allocate_resource(ResourceRequest {
            requester_id,
            resource_type: ResourceType::Cpu,
            amount: cores,
            priority: 0,
            timeout_ms: self.constraints.resource_timeout_ms,
            requested_at: Instant::now(),
        }).map(|grant| grant.grant_id)
    }

    /// Deallocate CPU cores
    pub fn deallocate_cpu(&self, grant_id: u64) -> Result<()> {
        self.deallocate_resource(grant_id)
    }

    /// Start an I/O operation
    pub fn start_io_operation(&self, requester_id: u64) -> Result<u64> {
        self.allocate_resource(ResourceRequest {
            requester_id,
            resource_type: ResourceType::Io,
            amount: 1,
            priority: 0,
            timeout_ms: self.constraints.resource_timeout_ms,
            requested_at: Instant::now(),
        }).map(|grant| grant.grant_id)
    }

    /// Complete an I/O operation
    pub fn complete_io_operation(&self, grant_id: u64) -> Result<()> {
        self.deallocate_resource(grant_id)
    }

    /// Check if resources are available for allocation
    pub fn check_availability(&self, resource_type: ResourceType, amount: usize) -> bool {
        match resource_type {
            ResourceType::Memory => {
                let tracker = self.memory_tracker.lock().unwrap();
                let current_usage = *tracker.get_usage();
                current_usage + amount <= self.constraints.max_total_memory
            }
            ResourceType::Cpu => {
                let tracker = self.cpu_tracker.lock().unwrap();
                let active_cores = tracker.get_active_cores();
                active_cores + amount <= self.constraints.max_concurrent_workers
            }
            ResourceType::Io => {
                let tracker = self.io_tracker.lock().unwrap();
                let active_ops = tracker.get_active_operations();
                active_ops + amount <= self.constraints.max_concurrent_io
            }
            _ => true, // Other resource types not yet implemented
        }
    }

    /// Get current resource usage statistics
    pub fn get_usage_stats(&self) -> HashMap<ResourceType, ResourceUsage> {
        self.usage.lock().unwrap().clone()
    }

    /// Get memory usage statistics
    pub fn get_memory_usage(&self) -> (usize, usize) {
        let tracker = self.memory_tracker.lock().unwrap();
        (*tracker.get_usage(), tracker.get_peak_usage())
    }

    /// Get CPU usage statistics
    pub fn get_cpu_usage(&self) -> (usize, f64) {
        let mut tracker = self.cpu_tracker.lock().unwrap();
        tracker.update_utilization();
        (tracker.get_active_cores(), tracker.get_utilization())
    }

    /// Get I/O usage statistics
    pub fn get_io_usage(&self) -> (usize, usize) {
        let tracker = self.io_tracker.lock().unwrap();
        (tracker.get_active_operations(), tracker.get_peak_operations())
    }

    /// Update resource constraints
    pub fn update_constraints(&mut self, constraints: ResourceConstraints) {
        self.constraints = constraints;
        // Notify waiting threads that constraints have changed
        self.resource_cv.notify_all();
    }

    /// Get resource constraints
    pub fn constraints(&self) -> &ResourceConstraints {
        &self.constraints
    }

    /// Shutdown the resource manager
    pub fn shutdown(mut self) -> Result<()> {
        // Set shutdown flag
        *self.shutdown.lock().unwrap() = true;

        // Wake up any waiting threads
        self.resource_cv.notify_all();

        // Join cleanup thread
        if let Some(handle) = self.cleanup_handle.take() {
            handle.join().map_err(|_| {
                crate::error::RustgreSQLError::Internal("Failed to join cleanup thread".to_string())
            })?;
        }

        Ok(())
    }

    /// Internal method to allocate a resource
    fn allocate_resource(&self, request: ResourceRequest) -> Result<ResourceGrant> {
        let start_time = Instant::now();
        let timeout = Duration::from_millis(request.timeout_ms);

        // Check if allocation is immediately possible
        if self.check_availability(request.resource_type, request.amount) {
            return self.grant_resource(request);
        }

        // Add to pending requests and wait
        {
            let mut pending = self.pending_requests.lock().unwrap();
            pending.push(request.clone());
        }

        let deadline = start_time + timeout;
        while Instant::now() < deadline {
            // Check if allocation is now possible
            if self.check_availability(request.resource_type, request.amount) {
                // Remove from pending and grant
                {
                    let mut pending = self.pending_requests.lock().unwrap();
                    if let Some(pos) = pending.iter().position(|r| r.requester_id == request.requester_id) {
                        pending.remove(pos);
                    }
                }
                return self.grant_resource(request);
            }

            // Wait for resource availability
            let mut pending_guard = self.pending_requests.lock().unwrap();
            let wait_result = self.resource_cv.wait_timeout(pending_guard, timeout).unwrap();

            if wait_result.timed_out() {
                break;
            }
        }

        // Timeout occurred, remove from pending and fail
        {
            let mut pending = self.pending_requests.lock().unwrap();
            if let Some(pos) = pending.iter().position(|r| r.requester_id == request.requester_id) {
                pending.remove(pos);
            }
        }

        // Update failure statistics
        {
            let mut usage = self.usage.lock().unwrap();
            if let Some(resource_usage) = usage.get_mut(&request.resource_type) {
                resource_usage.allocation_failures += 1;
            }
        }

        Err(crate::error::RustgreSQLError::Internal(
            format!("Resource allocation timed out for {:?} amount {}", request.resource_type, request.amount)
        ))
    }

    /// Internal method to grant a resource
    fn grant_resource(&self, request: ResourceRequest) -> Result<ResourceGrant> {
        let grant_id = {
            let mut next_id = self.next_grant_id.lock().unwrap();
            let id = *next_id;
            *next_id = id.wrapping_add(1);
            id
        };

        let grant = ResourceGrant {
            grant_id,
            requester_id: request.requester_id,
            resource_type: request.resource_type,
            amount: request.amount,
            granted_at: Instant::now(),
            expires_at: None, // No expiration by default
        };

        // Update usage tracking
        {
            let mut usage = self.usage.lock().unwrap();
            if let Some(resource_usage) = usage.get_mut(&request.resource_type) {
                resource_usage.allocated += request.amount;
                resource_usage.in_use += request.amount;
                resource_usage.peak_usage = resource_usage.peak_usage.max(resource_usage.in_use);
                resource_usage.active_allocations += 1;
                resource_usage.total_allocations += 1;
            }
        }

        // Update specific resource trackers
        match request.resource_type {
            ResourceType::Memory => {
                let mut tracker = self.memory_tracker.lock().unwrap();
                tracker.allocate(request.amount);
            }
            ResourceType::Cpu => {
                let mut tracker = self.cpu_tracker.lock().unwrap();
                tracker.allocate_cores(request.amount);
            }
            ResourceType::Io => {
                let mut tracker = self.io_tracker.lock().unwrap();
                tracker.start_operation();
            }
            _ => {} // Other resource types not yet implemented
        }

        // Store active grant
        {
            let mut active = self.active_grants.lock().unwrap();
            active.insert(grant_id, grant.clone());
        }

        Ok(grant)
    }

    /// Internal method to deallocate a resource
    fn deallocate_resource(&self, grant_id: u64) -> Result<()> {
        let grant = {
            let mut active = self.active_grants.lock().unwrap();
            active.remove(&grant_id).ok_or_else(|| {
                crate::error::RustgreSQLError::Internal(
                    format!("Resource grant {} not found", grant_id)
                )
            })?
        };

        // Update usage tracking
        {
            let mut usage = self.usage.lock().unwrap();
            if let Some(resource_usage) = usage.get_mut(&grant.resource_type) {
                resource_usage.in_use = resource_usage.in_use.saturating_sub(grant.amount);
                resource_usage.active_allocations = resource_usage.active_allocations.saturating_sub(1);
            }
        }

        // Update specific resource trackers
        match grant.resource_type {
            ResourceType::Memory => {
                let mut tracker = self.memory_tracker.lock().unwrap();
                tracker.deallocate(grant_id);
            }
            ResourceType::Cpu => {
                let mut tracker = self.cpu_tracker.lock().unwrap();
                tracker.release_cores(grant.amount);
            }
            ResourceType::Io => {
                let mut tracker = self.io_tracker.lock().unwrap();
                tracker.complete_operation();
            }
            _ => {} // Other resource types not yet implemented
        }

        // Notify waiting threads that resources may be available
        self.resource_cv.notify_all();

        Ok(())
    }

    /// Background cleanup thread
    fn cleanup_thread(
        memory_tracker: Arc<Mutex<MemoryTracker>>,
        usage: Arc<Mutex<HashMap<ResourceType, ResourceUsage>>>,
        shutdown: Arc<Mutex<bool>>,
    ) {
        loop {
            // Check shutdown flag
            if *shutdown.lock().unwrap() {
                break;
            }

            // Periodic cleanup and maintenance
            {
                let mut tracker = memory_tracker.lock().unwrap();
                let mut usage_guard = usage.lock().unwrap();

                // Update memory usage statistics
                if let Some(memory_usage) = usage_guard.get_mut(&ResourceType::Memory) {
                    memory_usage.in_use = *tracker.get_usage();
                }
            }

            // Sleep for cleanup interval
            thread::sleep(Duration::from_secs(5));
        }
    }
}

impl Drop for ResourceManager {
    fn drop(&mut self) {
        // Ensure shutdown is called
        let _ = self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_constraints_default() {
        let constraints = ResourceConstraints::default();
        assert!(constraints.max_memory_per_worker > 0);
        assert!(constraints.max_total_memory > 0);
        assert!(constraints.max_concurrent_workers > 0);
    }

    #[test]
    fn test_memory_tracker() {
        let mut tracker = MemoryTracker::new();

        let alloc1 = tracker.allocate(1024);
        assert!(alloc1.is_some());
        assert_eq!(*tracker.get_usage(), 1024);

        let alloc2 = tracker.allocate(2048);
        assert!(alloc2.is_some());
        assert_eq!(*tracker.get_usage(), 3072);

        assert!(tracker.deallocate(alloc1.unwrap()));
        assert_eq!(*tracker.get_usage(), 2048);

        assert!(!tracker.deallocate(999)); // Non-existent allocation
    }

    #[test]
    fn test_cpu_tracker() {
        let mut tracker = CpuTracker::new();

        assert!(tracker.allocate_cores(2));
        assert_eq!(tracker.get_active_cores(), 2);

        tracker.release_cores(1);
        assert_eq!(tracker.get_active_cores(), 1);

        tracker.update_utilization();
        assert!(tracker.get_utilization() >= 0.0 && tracker.get_utilization() <= 1.0);
    }

    #[test]
    fn test_io_tracker() {
        let mut tracker = IoTracker::new();

        assert!(tracker.start_operation());
        assert_eq!(tracker.get_active_operations(), 1);

        assert!(tracker.start_operation());
        assert_eq!(tracker.get_active_operations(), 2);
        assert_eq!(tracker.get_peak_operations(), 2);

        tracker.complete_operation();
        assert_eq!(tracker.get_active_operations(), 1);
    }

    #[test]
    fn test_resource_manager_creation() {
        let constraints = ResourceConstraints::default();
        let manager = ResourceManager::new(constraints);
        assert!(manager.is_ok());
    }

    #[test]
    fn test_resource_availability() {
        let constraints = ResourceConstraints {
            max_total_memory: 1024 * 1024, // 1MB
            ..Default::default()
        };
        let manager = ResourceManager::new(constraints).unwrap();

        assert!(manager.check_availability(ResourceType::Memory, 512 * 1024));
        assert!(manager.check_availability(ResourceType::Memory, 1024 * 1024));
        assert!(!manager.check_availability(ResourceType::Memory, 2 * 1024 * 1024));
    }
}