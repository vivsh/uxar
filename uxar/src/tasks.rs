use std::collections::VecDeque;



pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}


pub struct Task {
    pub id: uuid::Uuid,
    pub service: uuid::Uuid,
    pub input: serde_json::Value,
    pub status: TaskStatus,
    pub next_time: chrono::DateTime<chrono::Utc>,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub status_time: Option<chrono::DateTime<chrono::Utc>>,
}



pub trait TaskStore{

}


pub struct TaskEngine{
    queue: VecDeque<Task>,
}

impl TaskEngine{

    pub fn new() -> Self{
        Self{
            queue: VecDeque::new(),
        }
    }

    pub fn add_task(&mut self, task: Task){
        self.queue.push_back(task);
    }

    pub fn get_next_task(&mut self) -> Option<Task>{
        self.queue.pop_front()
    }

    pub fn run(mut self){
        while let Some(mut task) = self.get_next_task(){
            // Process the task
            println!("Processing task: {:?}", task.id);
            task.status = TaskStatus::Completed;
            task.status_time = Some(chrono::Utc::now());
            // In a real implementation, you would handle failures, retries, etc.
        }
    }
}