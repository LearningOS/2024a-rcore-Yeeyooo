//! Process management syscalls
use crate::{
    config::MAX_SYSCALL_NUM,
    task::{exit_current_and_run_next, suspend_current_and_run_next, get_syscall_times, get_status, get_current_running_time, TaskStatus},
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// Task information
/// 表示正在执行的任务信息
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,   // 正在执行的任务的任务状态
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],   // 任务使用的系统调用及调用次数, 在实验中系统调用号一定小于500， 所以使用一个长为MAX_SYSCALL_NUM=500的数组做桶计数
    /// Total running time of task
    time: usize,          // 系统调用时刻距离任务第一次被调度时刻的时长(单位: ms), 这个时长可能包含该任务被其他任务抢占后的等待重新调度的时间
}

/// task exits and submit an exit code
/// 程序主动退出sys_exit, 基于task子模块提供的exit_current_and_run_next接口
/// 含义是退出当前的应用并且切换到下一个应用
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
/// 功能:程序主动暂停sys_yield，应用主动交出CPU所有权并且切换到其他应用
/// 返回值: 总是返回0
/// syscall ID: 124
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// get time with second and microsecond
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    unsafe {
        *ts = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,
        };
    }
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// 查询正在执行的任务信息，任务信息包括任务控制块的相关信息(任务状态)，任务使用的系统调用和系统调用次数
/// 系统调用时刻距离任务第一次被调度时刻的时长(单位ms)
/// 参数ti: 待查询的任务信息
/// 返回值: 执行成功返回0, 错误返回-1
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info");
    unsafe {
        *_ti = TaskInfo {
            status: get_status(),       // 由于查询的是当前任务的状态，所以查询的TaskStatus一定是Running
            syscall_times: get_syscall_times(),
            time: get_current_running_time(),
        }
    }
    0
}
