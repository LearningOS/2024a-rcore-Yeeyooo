//! Types related to task management

use crate::config::MAX_SYSCALL_NUM;

use super::TaskContext;

/// The task control block (TCB) of a task.
/// 维护任务状态和任务上下文, 两者一并保存在任务控制块的数据结构中
/// 任务控制块非常重要，在内核中，任务控制块就是应用的管理单位
/// 在内核中，任务控制块就是应用的管理单位
#[derive(Copy, Clone)]
pub struct TaskControlBlock {
    /// The task status in it's lifecycle
    pub task_status: TaskStatus,
    /// The task context
    pub task_cx: TaskContext,
    /// 记录当前任务的系统调用的次数
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    /// 当前任务首次被调度的时间, 通过使用Option记录该任务是否是首次被调度
    pub first_time: Option<usize>,
}

/// The status of a task
/// 任务运行状态: 未初始化、准备执行、正在执行、已退出
#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    /// uninitialized
    UnInit,
    /// ready to run
    Ready,
    /// running
    Running,
    /// exited
    Exited,
}
