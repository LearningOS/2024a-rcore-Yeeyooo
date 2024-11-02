//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the operating system.
//!
//! Be careful when you see `__switch` ASM function in `switch.S`. Control flow around this function
//! might not be what you expect.

mod context;
mod switch;
#[allow(clippy::module_inception)]
mod task;

use crate::config::{MAX_APP_NUM, MAX_SYSCALL_NUM};
use crate::loader::{get_num_app, init_app_cx};
use crate::sync::UPSafeCell;
use crate::timer::get_time_ms;
use lazy_static::*;
use switch::__switch;
pub use task::{TaskControlBlock, TaskStatus};

pub use context::TaskContext;

/// The task manager, where all the tasks are managed.
///
/// Functions implemented on `TaskManager` deals with all task state transitions
/// and task context switching. For convenience, you can find wrappers around it
/// in the module level.
///
/// Most of `TaskManager` are hidden behind the field `inner`, to defer
/// borrowing checks to runtime. You can see examples on how to use `inner` in
/// existing functions on `TaskManager`.
/// 这里使用到了变量和常量分离的编程风格, num_app表示应用的数量, 在TaskManager初始化之后将保持不变
/// 而包裹在TaskManagerInner中的任务控制块数量tasks和正在执行的应用编号current_task会在执行中发生变化
pub struct TaskManager {
    /// total number of tasks
    num_app: usize,
    /// use inner value to get mutable access
    inner: UPSafeCell<TaskManagerInner>,
}

/// Inner of Task Manager
pub struct TaskManagerInner {
    /// task list, 包含每个任务对应的TaskControlBlock
    tasks: [TaskControlBlock; MAX_APP_NUM],
    /// id of current `Running` task
    current_task: usize,
}

lazy_static! {
    /// Global variable: TASK_MANAGER
    /// 初始化TaskManager的全局实例TASK_MANAGER
    pub static ref TASK_MANAGER: TaskManager = {
        let num_app = get_num_app();  // 调用loader子模块提供的get_num_app接口获取链接到内核的应用总数
        let mut tasks = [TaskControlBlock {
            task_cx: TaskContext::zero_init(),
            task_status: TaskStatus::UnInit,
            syscall_times: [0; MAX_SYSCALL_NUM],   // 将当前任务的系统调用的次数都初始化为0
            first_time: None,                      // None表示当前任务还没有被调度过
        }; MAX_APP_NUM];
        // 依次对每个任务控制块进行初始化，将运行状态设置为Ready, 并且在其内核栈栈顶压入一些初始化上下文
        // 然后更新它的task_cx
        // init_app_cx在loader子模块中定义，向内核栈压入了一个Trap上下文，并且返回压入Trap上下文后sp的值
        // goto_restore保存传入的sp, 并将ra设置为_restore的入口地址，构造任务上下文后返回，这样任务管理器
        // 中各个应用的任务上下文就得到了初始化
        for (i, task) in tasks.iter_mut().enumerate() {
            task.task_cx = TaskContext::goto_restore(init_app_cx(i));
            task.task_status = TaskStatus::Ready;
        }
        TaskManager {
            num_app,
            inner: unsafe {
                UPSafeCell::new(TaskManagerInner {
                    tasks,
                    current_task: 0,
                })
            },
        }
    };
}

impl TaskManager {
    fn syscall_count(&self, syscall_id: usize) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;      // 从TaskManagerInner中获取当前运行的任务的编号
        inner.tasks[current].syscall_times[syscall_id] += 1;   // 对应系统调用的计数+1
    }

    /// Run the first task in task list.
    ///
    /// Generally, the first task in task list is an idle task (we call it zero process later).
    /// But in ch3, we load apps statically, so the first task is a real app.
    fn run_first_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let task0 = &mut inner.tasks[0];
        task0.task_status = TaskStatus::Running;
        let next_task_cx_ptr = &task0.task_cx as *const TaskContext;

        task0.first_time = Some(get_time_ms());       // 记录首次被调度的时间
        drop(inner);
        let mut _unused = TaskContext::zero_init();
        // before this, we should drop local variables that must be dropped manually
        unsafe {
            __switch(&mut _unused as *mut TaskContext, next_task_cx_ptr);
        }
        panic!("unreachable in run_first_task!");
    }

    /// Change the status of current `Running` task into `Ready`.
    /// 先获得里层TaskManagerInner的可变引用，然后修改任务控制块数组tasks中当前任务的状态
    fn mark_current_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;  // current是当前运行的任务的编号
        inner.tasks[current].task_status = TaskStatus::Ready; // 修改当前运行的任务的状态
    }

    /// Change the status of current `Running` task into `Exited`.
    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;  // current是当前运行的任务的编号
        inner.tasks[current].task_status = TaskStatus::Exited; // 修改当前运行的任务的状态
    }

    /// Find next task to run and return task id.
    ///
    /// In this case, we only return the first `Ready` task in task list.
    fn find_next_task(&self) -> Option<usize> {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;   // current是当前运行的任务的编号, 从TaskManagerInner中获取当前运行的任务的编号
        (current + 1..current + self.num_app + 1)   // 在当前编号范围内寻找下一个状态是TaskStatus::Ready的
            .map(|id| id % self.num_app)
            .find(|id| inner.tasks[*id].task_status == TaskStatus::Ready)
    }

    /// Switch current `Running` task to the task we have found,
    /// or there is no `Ready` task and we can exit with all applications completed
    fn run_next_task(&self) {
        if let Some(next) = self.find_next_task() {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;       // 从TaskManagerInner中获取当前运行的任务的编号
            inner.tasks[next].task_status = TaskStatus::Running;
            inner.current_task = next;
            let current_task_cx_ptr = &mut inner.tasks[current].task_cx as *mut TaskContext;
            let next_task_cx_ptr = &inner.tasks[next].task_cx as *const TaskContext;

            // 如果要运行的下个任务是首次被调度，记录首次被调度的时间
            if inner.tasks[next].first_time.is_none() {
                inner.tasks[next].first_time = Some(get_time_ms());
            }
            drop(inner);
            // before this, we should drop local variables that must be dropped manually
            unsafe {
                __switch(current_task_cx_ptr, next_task_cx_ptr);
            }
            // go back to user mode
        } else {
            panic!("All applications completed!");
        }
    }

    /// 获取当前正在运行任务的系统调用次数信息
    fn get_syscall_times(&self) -> [u32; MAX_SYSCALL_NUM] {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].syscall_times.clone()
    }

    fn get_status(&self) -> TaskStatus {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status
    }

    fn get_current_running_time(&self) -> usize {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        // 计算当前时间和当前运行任务首次运行的时间差
        get_time_ms() - inner.tasks[current].first_time.unwrap()
    }
}

/// Run the first task in task list.
pub fn run_first_task() {
    TASK_MANAGER.run_first_task();
}

/// Switch current `Running` task to the task we have found,
/// or there is no `Ready` task and we can exit with all applications completed
fn run_next_task() {
    TASK_MANAGER.run_next_task();
}

/// Change the status of current `Running` task into `Ready`.
fn mark_current_suspended() {
    TASK_MANAGER.mark_current_suspended();
}

/// Change the status of current `Running` task into `Exited`.
fn mark_current_exited() {
    TASK_MANAGER.mark_current_exited();
}

/// Suspend the current 'Running' task and run the next task in task list.
pub fn suspend_current_and_run_next() {
    mark_current_suspended();
    run_next_task();
}

/// Exit the current 'Running' task and run the next task in task list.
pub fn exit_current_and_run_next() {
    mark_current_exited();
    run_next_task();
}

/// 维护TaskManager中当前运行的任务的系统调用计数
pub fn record_syscall_times(syscall_id: usize) {
    TASK_MANAGER.syscall_count(syscall_id);
}

/// 获取当前运行任务的系统调用次数信息
pub fn get_syscall_times() -> [u32; MAX_SYSCALL_NUM] {
    TASK_MANAGER.get_syscall_times()
}

/// 获取当前正在运行任务的状态
pub fn get_status() -> TaskStatus {
    TASK_MANAGER.get_status()
}

/// 获取当前正在运行的任务距离第一次被调度的时长(单位: ms)
pub fn get_current_running_time() -> usize {
    TASK_MANAGER.get_current_running_time()
}