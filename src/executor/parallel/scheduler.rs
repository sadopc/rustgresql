//! Parallel task scheduling system
//!
//! Provides work-stealing task scheduling optimized for query workloads.

use crate::Result;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Unique identifier for parallel tasks
pub type TaskId = u64;

/// Types of parallel tasks that can be scheduled
#[derive(Debug, Clone, PartialEq)]
pub enum TaskType {
    /// Sequential scan of data range
    ScanRange,
    /// Filter operation on data chunk
    Filter,
    /// Hash table build for join
    HashBuild,
    /// Hash table probe for join
    HashProbe,
    /// Partial aggregation
    PartialAggregate,
    /// Final aggregation merge
    FinalAggregate,
    /// Sort operation on data chunk
    Sort,
    /// Generic computation task
    Compute,
}

/// Represents a parallel task to be executed
#[derive(Debug)]
pub struct ParallelTask {
    /// Unique task identifier
    pub id: TaskId,
    /// Type of task
    pub task_type: TaskType,
    /// Task priority (lower = higher priority)
    pub priority: u32,
    /// Estimated execution cost
    pub estimated_cost: f64,
    /// Task-specific data
    pub data: Vec<u8>,
    /// Dependencies on other tasks
    pub dependencies: Vec<TaskId>,
    /// Creation timestamp
    pub created_at: Instant,
    /// Worker affinity hint (optional)
    pub worker_affinity: Option<usize>,
}

impl ParallelTask {
    /// Create a new parallel task
    pub fn new(
        id: TaskId,
        task_type: TaskType,
        priority: u32,
        estimated_cost: f64,
        data: Vec<u8>,
    ) -> Self {
        Self {
            id,
            task_type,
            priority,
            estimated_cost,
            data,
            dependencies: Vec::new(),
            created_at: Instant::now(),
            worker_affinity: None,
        }
    }

    /// Add dependency on another task
    pub fn add_dependency(&mut self, dependency: TaskId) {
        self.dependencies.push(dependency);
    }

    /// Set worker affinity hint
    pub fn set_worker_affinity(&mut self, worker_id: usize) {
        self.worker_affinity = Some(worker_id);
    }

    /// Check if task has any dependencies
    pub fn has_dependencies(&self) -> bool {
        !self.dependencies.is_empty()
    }
}

/// Task execution result
#[derive(Debug)]
pub struct TaskResult {
    /// Task identifier
    pub task_id: TaskId,
    /// Execution result data
    pub result: Vec<u8>,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Success flag
    pub success: bool,
    /// Error message if execution failed
    pub error: Option<String>,
    /// Memory usage in bytes
    pub memory_used: usize,
}

/// Work-stealing task scheduler
pub struct TaskScheduler {
    /// Global task queue
    global_queue: Arc<Mutex<VecDeque<ParallelTask>>>,
    /// Local queues for each worker
    local_queues: Vec<Arc<Mutex<VecDeque<ParallelTask>>>>,
    /// Worker threads
    workers: Vec<WorkerThread>,
    /// Completed tasks
    completed_tasks: Arc<Mutex<std::collections::HashMap<TaskId, TaskResult>>>,
    /// Pending dependencies tracking
    pending_tasks: Arc<Mutex<std::collections::HashMap<TaskId, Vec<TaskId>>>>,
    /// Scheduler shutdown flag
    shutdown: Arc<Mutex<bool>>,
    /// Task execution callback
    task_executor: Arc<dyn Fn(&ParallelTask) -> TaskResult + Send + Sync>,
    /// Next task ID
    next_task_id: Arc<Mutex<TaskId>>,
    /// Work-stealing interval
    steal_interval: Duration,
    /// Load balancer
    load_balancer: Arc<Mutex<LoadBalancer>>,
}

/// Worker thread information
struct WorkerThread {
    handle: JoinHandle<()>,
    worker_id: usize,
}

/// Load balancer for distributing tasks
struct LoadBalancer {
    /// Tasks completed by each worker
    worker_tasks: Vec<usize>,
    /// Total execution time per worker
    worker_time: Vec<u64>,
    /// Last rebalance time
    last_rebalance: Instant,
}

impl LoadBalancer {
    fn new(num_workers: usize) -> Self {
        Self {
            worker_tasks: vec![0; num_workers],
            worker_time: vec![0; num_workers],
            last_rebalance: Instant::now(),
        }
    }

    fn record_task_completion(&mut self, worker_id: usize, execution_time_ms: u64) {
        if worker_id < self.worker_tasks.len() {
            self.worker_tasks[worker_id] += 1;
            self.worker_time[worker_id] += execution_time_ms;
        }
    }

    fn get_load_imbalance(&self) -> f64 {
        if self.worker_tasks.is_empty() {
            return 0.0;
        }

        let total_tasks: usize = self.worker_tasks.iter().sum();
        if total_tasks == 0 {
            return 0.0;
        }

        let avg_tasks = total_tasks as f64 / self.worker_tasks.len() as f64;
        let variance: f64 = self.worker_tasks
            .iter()
            .map(|&tasks| (tasks as f64 - avg_tasks).powi(2))
            .sum::<f64>() / self.worker_tasks.len() as f64;

        variance.sqrt() / avg_tasks.max(1.0)
    }

    fn should_rebalance(&self) -> bool {
        self.last_rebalance.elapsed() > Duration::from_secs(5) && self.get_load_imbalance() > 0.3
    }
}

impl TaskScheduler {
    /// Create a new task scheduler
    pub fn new<F>(num_workers: usize, task_executor: F) -> Result<Self>
    where
        F: Fn(&ParallelTask) -> TaskResult + Send + Sync + 'static,
    {
        let global_queue = Arc::new(Mutex::new(VecDeque::new()));
        let mut local_queues = Vec::new();
        let completed_tasks = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let pending_tasks = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let shutdown = Arc::new(Mutex::new(false));
        let next_task_id = Arc::new(Mutex::new(1));
        let load_balancer = Arc::new(Mutex::new(LoadBalancer::new(num_workers)));

        // Create local queues for each worker
        for _ in 0..num_workers {
            local_queues.push(Arc::new(Mutex::new(VecDeque::new())));
        }

        let task_executor = Arc::new(task_executor);
        let mut workers = Vec::new();

        // Spawn worker threads
        for worker_id in 0..num_workers {
            let global_queue = Arc::clone(&global_queue);
            let local_queue = Arc::clone(&local_queues[worker_id]);
            let completed_tasks = Arc::clone(&completed_tasks);
            let pending_tasks = Arc::clone(&pending_tasks);
            let shutdown = Arc::clone(&shutdown);
            let task_executor = Arc::clone(&task_executor);
            let load_balancer = Arc::clone(&load_balancer);

            let handle = thread::Builder::new()
                .name(format!("rustgresql-worker-{}", worker_id))
                .spawn(move || {
                    Self::worker_loop(
                        worker_id,
                        global_queue,
                        local_queue,
                        completed_tasks,
                        pending_tasks,
                        shutdown,
                        task_executor,
                        load_balancer,
                    )
                })?;

            workers.push(WorkerThread { handle, worker_id });
        }

        Ok(Self {
            global_queue,
            local_queues,
            workers,
            completed_tasks,
            pending_tasks,
            shutdown,
            task_executor,
            next_task_id,
            steal_interval: Duration::from_millis(100),
            load_balancer,
        })
    }

    /// Schedule a task for execution
    pub fn schedule_task(&mut self, mut task: ParallelTask) -> Result<TaskId> {
        let task_id = task.id;

        // Add to pending dependencies tracking
        if task.has_dependencies() {
            let mut pending = self.pending_tasks.lock().unwrap();
            for dep_id in &task.dependencies {
                pending.entry(*dep_id).or_insert_with(Vec::new).push(task_id);
            }
        }

        // Determine which queue to use
        let target_worker = task.worker_affinity.unwrap_or(0);
        if task.worker_affinity.is_some() && target_worker < self.local_queues.len() {
            // Use specific worker's local queue
            let local_queue = &self.local_queues[target_worker];
            let mut queue = local_queue.lock().unwrap();
            self.insert_task_sorted(&mut queue, task);
        } else {
            // Use global queue
            let mut global = self.global_queue.lock().unwrap();
            self.insert_task_sorted(&mut global, task);
        }

        Ok(task_id)
    }

    /// Create and schedule a new task
    pub fn submit_task(
        &mut self,
        task_type: TaskType,
        priority: u32,
        estimated_cost: f64,
        data: Vec<u8>,
    ) -> Result<TaskId> {
        let task_id = self.generate_task_id();
        let task = ParallelTask::new(task_id, task_type, priority, estimated_cost, data);
        self.schedule_task(task)
    }

    /// Wait for task completion
    pub fn wait_for_task(&self, task_id: TaskId, timeout_ms: Option<u64>) -> Result<TaskResult> {
        let start_time = Instant::now();
        let timeout = timeout_ms.map(Duration::from_millis);

        loop {
            {
                let completed = self.completed_tasks.lock().unwrap();
                if let Some(result) = completed.get(&task_id) {
                    return Ok(result.clone());
                }
            }

            // Check timeout
            if let Some(timeout_duration) = timeout {
                if start_time.elapsed() > timeout_duration {
                    return Err(crate::error::RustgreSQLError::Internal(
                        format!("Task {} timed out after {}ms", task_id, timeout_ms.unwrap())
                    ));
                }
            }

            // Brief sleep to avoid busy waiting
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Wait for multiple tasks to complete
    pub fn wait_for_tasks(&self, task_ids: &[TaskId], timeout_ms: Option<u64>) -> Result<Vec<TaskResult>> {
        let mut results = Vec::new();
        let start_time = Instant::now();
        let timeout = timeout_ms.map(Duration::from_millis);

        loop {
            let completed = self.completed_tasks.lock().unwrap();
            let mut all_completed = true;

            for &task_id in task_ids {
                if let Some(result) = completed.get(&task_id) {
                    results.push(result.clone());
                } else {
                    all_completed = false;
                }
            }

            if all_completed {
                return Ok(results);
            }

            // Check timeout
            if let Some(timeout_duration) = timeout {
                if start_time.elapsed() > timeout_duration {
                    return Err(crate::error::RustgreSQLError::Internal(
                        format!("Tasks timed out after {}ms", timeout_ms.unwrap())
                    ));
                }
            }

            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Cancel a task (if not yet started)
    pub fn cancel_task(&self, task_id: TaskId) -> Result<bool> {
        // Try to remove from global queue
        {
            let mut global = self.global_queue.lock().unwrap();
            if let Some(pos) = global.iter().position(|task| task.id == task_id) {
                global.remove(pos);
                return Ok(true);
            }
        }

        // Try to remove from local queues
        for local_queue in &self.local_queues {
            let mut queue = local_queue.lock().unwrap();
            if let Some(pos) = queue.iter().position(|task| task.id == task_id) {
                queue.remove(pos);
                return Ok(true);
            }
        }

        Ok(false) // Task not found (likely already executing)
    }

    /// Get scheduler statistics
    pub fn get_stats(&self) -> SchedulerStats {
        let completed_count = self.completed_tasks.lock().unwrap().len();
        let pending_count = self.global_queue.lock().unwrap().len();
        let local_pending: usize = self.local_queues
            .iter()
            .map(|q| q.lock().unwrap().len())
            .sum();

        SchedulerStats {
            pending_tasks: pending_count + local_pending,
            completed_tasks: completed_count,
            active_workers: self.workers.len(),
            load_imbalance: self.load_balancer.lock().unwrap().get_load_imbalance(),
        }
    }

    /// Shutdown the scheduler
    pub fn shutdown(mut self) -> Result<()> {
        // Set shutdown flag
        *self.shutdown.lock().unwrap() = true;

        // Join all worker threads
        for worker in self.workers.drain(..) {
            worker.handle.join().map_err(|_| {
                crate::error::RustgreSQLError::Internal("Failed to join worker thread".to_string())
            })?;
        }

        Ok(())
    }

    /// Worker thread main loop
    fn worker_loop(
        worker_id: usize,
        global_queue: Arc<Mutex<VecDeque<ParallelTask>>>,
        local_queue: Arc<Mutex<VecDeque<ParallelTask>>>,
        completed_tasks: Arc<Mutex<std::collections::HashMap<TaskId, TaskResult>>>,
        pending_tasks: Arc<Mutex<std::collections::HashMap<TaskId, Vec<TaskId>>>>,
        shutdown: Arc<Mutex<bool>>,
        task_executor: Arc<dyn Fn(&ParallelTask) -> TaskResult + Send + Sync>,
        load_balancer: Arc<Mutex<LoadBalancer>>,
    ) {
        loop {
            // Check shutdown flag
            if *shutdown.lock().unwrap() {
                break;
            }

            // Try to get a task from local queue
            let task = {
                let mut local = local_queue.lock().unwrap();
                local.pop_front()
            };

            let task = task.or_else(|| {
                // Try global queue
                let mut global = global_queue.lock().unwrap();
                global.pop_front()
            }).or_else(|| {
                // Try to steal from other workers
                Self::try_steal_task(worker_id, &global_queue)
            });

            if let Some(task) = task {
                // Check if dependencies are satisfied
                if Self::dependencies_satisfied(&task, &completed_tasks) {
                    // Execute the task
                    let start_time = Instant::now();
                    let result = (task_executor)(&task);
                    let execution_time = start_time.elapsed().as_millis() as u64;

                    // Record completion
                    {
                        let mut completed = completed_tasks.lock().unwrap();
                        completed.insert(task.id, result.clone());
                    }

                    // Update load balancer
                    {
                        let mut balancer = load_balancer.lock().unwrap();
                        balancer.record_task_completion(worker_id, execution_time);

                        // Check if rebalancing is needed
                        if balancer.should_rebalance() {
                            Self::rebalance_tasks(&global_queue);
                            balancer.last_rebalance = Instant::now();
                        }
                    }

                    // Process dependent tasks
                    Self::process_dependent_tasks(task.id, &pending_tasks, &completed_tasks, &global_queue);
                } else {
                    // Dependencies not satisfied, put task back
                    let mut local = local_queue.lock().unwrap();
                    local.push_front(task);
                    thread::sleep(Duration::from_millis(10)); // Brief wait
                }
            } else {
                // No tasks available, wait briefly
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    /// Try to steal a task from another worker's queue
    fn try_steal_task(
        worker_id: usize,
        global_queue: &Arc<Mutex<VecDeque<ParallelTask>>>,
    ) -> Option<ParallelTask> {
        let mut global = global_queue.lock().unwrap();
        // Steal from the back of the global queue (work-stealing strategy)
        global.pop_back()
    }

    /// Check if task dependencies are satisfied
    fn dependencies_satisfied(
        task: &ParallelTask,
        completed_tasks: &Arc<Mutex<std::collections::HashMap<TaskId, TaskResult>>>,
    ) -> bool {
        if !task.has_dependencies() {
            return true;
        }

        let completed = completed_tasks.lock().unwrap();
        task.dependencies.iter().all(|dep_id| completed.contains_key(dep_id))
    }

    /// Process tasks that depend on completed task
    fn process_dependent_tasks(
        completed_task_id: TaskId,
        pending_tasks: &Arc<Mutex<std::collections::HashMap<TaskId, Vec<TaskId>>>>,
        completed_tasks: &Arc<Mutex<std::collections::HashMap<TaskId, TaskResult>>>,
        global_queue: &Arc<Mutex<VecDeque<ParallelTask>>>,
    ) {
        let dependent_tasks = {
            let mut pending = pending_tasks.lock().unwrap();
            pending.remove(&completed_task_id).unwrap_or_default()
        };

        for dependent_id in dependent_tasks {
            // Check if this dependent task is now ready
            // This is simplified - in practice we'd need to check all dependencies
            let mut global = global_queue.lock().unwrap();
            if let Some(pos) = global.iter().position(|task| task.id == dependent_id) {
                let task = global.remove(pos).unwrap();
                if Self::dependencies_satisfied(&task, completed_tasks) {
                    // Task is ready, move to front of queue
                    global.push_front(task);
                } else {
                    // Still has dependencies, put it back
                    global.push_back(task);
                }
            }
        }
    }

    /// Rebalance tasks across workers
    fn rebalance_tasks(global_queue: &Arc<Mutex<VecDeque<ParallelTask>>>) {
        // Simple rebalancing - move some tasks to make them more evenly distributed
        let mut global = global_queue.lock().unwrap();
        if global.len() > 10 {
            // Rotate the queue to distribute work
            let half = global.len() / 2;
            let mut moved_tasks = global.split_off(half);
            moved_tasks.reverse();
            global.extend(moved_tasks);
        }
    }

    /// Insert task into queue sorted by priority and estimated cost
    fn insert_task_sorted(queue: &mut VecDeque<ParallelTask>, task: ParallelTask) {
        let insert_pos = queue.iter().position(|existing_task| {
            task.priority < existing_task.priority ||
            (task.priority == existing_task.priority && task.estimated_cost > existing_task.estimated_cost)
        }).unwrap_or(queue.len());

        queue.insert(insert_pos, task);
    }

    /// Generate next task ID
    fn generate_task_id(&self) -> TaskId {
        let mut next_id = self.next_task_id.lock().unwrap();
        let id = *next_id;
        *next_id = id.wrapping_add(1);
        id
    }
}

/// Scheduler statistics
#[derive(Debug, Clone)]
pub struct SchedulerStats {
    pub pending_tasks: usize,
    pub completed_tasks: usize,
    pub active_workers: usize,
    pub load_imbalance: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_creation() {
        let task = ParallelTask::new(
            1,
            TaskType::ScanRange,
            0,
            100.0,
            vec![1, 2, 3],
        );

        assert_eq!(task.id, 1);
        assert_eq!(task.task_type, TaskType::ScanRange);
        assert_eq!(task.priority, 0);
        assert_eq!(task.estimated_cost, 100.0);
        assert!(!task.has_dependencies());
    }

    #[test]
    fn test_task_dependencies() {
        let mut task = ParallelTask::new(
            2,
            TaskType::Filter,
            1,
            50.0,
            vec![4, 5, 6],
        );

        task.add_dependency(1);
        assert!(task.has_dependencies());
        assert_eq!(task.dependencies, vec![1]);
    }

    #[test]
    fn test_load_balancer() {
        let mut balancer = LoadBalancer::new(4);

        balancer.record_task_completion(0, 100);
        balancer.record_task_completion(1, 150);
        balancer.record_task_completion(2, 75);
        balancer.record_task_completion(3, 125);

        assert_eq!(balancer.worker_tasks, vec![1, 1, 1, 1]);
        assert_eq!(balancer.worker_time, vec![100, 150, 75, 125]);
        assert!(balancer.get_load_imbalance() >= 0.0);
        assert!(!balancer.should_rebalance());
    }
}