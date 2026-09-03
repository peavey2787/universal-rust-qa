use std::{collections::HashMap, io, mem::size_of};

use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
        LibraryLoader::{GetModuleHandleA, GetProcAddress},
        Threading::{OpenProcess, PROCESS_SUSPEND_RESUME, PROCESS_TERMINATE, TerminateProcess},
    },
};

#[derive(Clone, Copy)]
struct ProcessEntry {
    pid: u32,
    parent_pid: u32,
}

#[derive(Clone, Copy)]
struct TreeMember {
    pid: u32,
    depth: usize,
}

struct ProcessTree {
    root_present: bool,
    members: Vec<TreeMember>,
}

#[derive(Clone, Copy)]
enum ProcessAction {
    Suspend,
    Resume,
    Terminate,
}

pub(super) fn suspend_process_tree(pid: u32) -> io::Result<()> {
    control_process_tree(pid, ProcessAction::Suspend, true)
}

pub(super) fn resume_process_tree(pid: u32) -> io::Result<()> {
    control_process_tree(pid, ProcessAction::Resume, true)
}

pub(super) fn terminate_process_tree(pid: u32) -> io::Result<()> {
    control_process_tree(pid, ProcessAction::Terminate, true)
}

pub(super) fn terminate_descendants(pid: u32) -> io::Result<()> {
    control_process_tree(pid, ProcessAction::Terminate, false)
}

fn control_process_tree(pid: u32, action: ProcessAction, include_root: bool) -> io::Result<()> {
    let mut tree = process_tree(pid)?;
    if include_root && !tree.root_present {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("process {pid} is no longer running"),
        ));
    }

    tree.members.retain(|member| include_root || member.pid != pid);
    if tree.members.is_empty() {
        return Ok(());
    }
    match action {
        ProcessAction::Suspend => tree.members.sort_by_key(|member| member.depth),
        ProcessAction::Resume | ProcessAction::Terminate => {
            tree.members.sort_by_key(|member| std::cmp::Reverse(member.depth));
        }
    }

    let mut changed = 0usize;
    let mut first_error = None;
    for member in tree.members {
        match apply_action(member.pid, action) {
            Ok(true) => changed += 1,
            Ok(false) => {}
            Err(error) => {
                if member.pid == pid && include_root {
                    return Err(error);
                }
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    match (changed, first_error) {
        (0, Some(error)) => Err(error),
        _ => Ok(()),
    }
}

fn process_tree(root_pid: u32) -> io::Result<ProcessTree> {
    let rows = process_snapshot()?;
    let root_present = rows.iter().any(|row| row.pid == root_pid);
    let mut depths = HashMap::from([(root_pid, 0usize)]);
    loop {
        let mut changed = false;
        for row in &rows {
            let Some(parent_depth) = depths.get(&row.parent_pid).copied() else {
                continue;
            };
            if depths.insert(row.pid, parent_depth + 1).is_none() {
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let members = depths.into_iter().map(|(pid, depth)| TreeMember { pid, depth }).collect();
    Ok(ProcessTree { root_present, members })
}

fn process_snapshot() -> io::Result<Vec<ProcessEntry>> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let snapshot = OwnedHandle(snapshot);
    let mut entry =
        PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    if unsafe { Process32FirstW(snapshot.0, &mut entry) } == 0 {
        return Err(io::Error::last_os_error());
    }

    let mut rows = Vec::new();
    loop {
        rows.push(ProcessEntry { pid: entry.th32ProcessID, parent_pid: entry.th32ParentProcessID });
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        if unsafe { Process32NextW(snapshot.0, &mut entry) } == 0 {
            break;
        }
    }
    Ok(rows)
}

fn apply_action(pid: u32, action: ProcessAction) -> io::Result<bool> {
    let access = match action {
        ProcessAction::Suspend | ProcessAction::Resume => PROCESS_SUSPEND_RESUME,
        ProcessAction::Terminate => PROCESS_TERMINATE,
    };
    let handle = unsafe { OpenProcess(access, 0, pid) };
    if handle.is_null() {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(87) {
            return Ok(false);
        }
        return Err(error);
    }
    let handle = OwnedHandle(handle);

    match action {
        ProcessAction::Suspend => nt_process_control(handle.0, b"NtSuspendProcess\0", "suspend"),
        ProcessAction::Resume => nt_process_control(handle.0, b"NtResumeProcess\0", "resume"),
        ProcessAction::Terminate => {
            if unsafe { TerminateProcess(handle.0, 1) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(true)
        }
    }
}

type NtProcessControl = unsafe extern "system" fn(HANDLE) -> i32;

fn nt_process_control(handle: HANDLE, symbol: &[u8], action: &str) -> io::Result<bool> {
    let module = unsafe { GetModuleHandleA(c"ntdll.dll".as_ptr().cast()) };
    if module.is_null() {
        return Err(io::Error::last_os_error());
    }
    let operation = unsafe { GetProcAddress(module, symbol.as_ptr()) };
    let Some(operation) = operation else {
        return Err(io::Error::last_os_error());
    };
    let operation = unsafe {
        std::mem::transmute::<unsafe extern "system" fn() -> isize, NtProcessControl>(operation)
    };
    let status = unsafe { operation(handle) };
    if status >= 0 {
        Ok(true)
    } else {
        Err(io::Error::other(format!(
            "unable to {action} process (NTSTATUS 0x{:08X})",
            status as u32
        )))
    }
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}
