//! One-shot Shell activation outside a terminal's job, using the current user's shell.
//! No worker survives activation and no process environment or job is modified.
use super::ffi::{CloseHandle, GetLastError, GetWindowThreadProcessId, Handle};
use crate::i18n::Failure;
use std::{
    ffi::{c_void, OsStr},
    io,
    marker::PhantomData,
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr::{null, null_mut},
};

const PROCESS_CREATE_PROCESS: u32 = 0x0080;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const TOKEN_QUERY: u32 = 0x0008;
const TOKEN_USER: u32 = 1;
const TOKEN_INTEGRITY_LEVEL: u32 = 25;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const PARENT_PROCESS: usize = 0x0002_0000;
const CREATE_FLAGS: u32 = 0x0008_0000 | 0x0800_0000 | 0x0000_0400;
const MAX_COMMAND_UNITS: usize = 32_767;
const MAX_ENVIRONMENT_UNITS: usize = 1024 * 1024;
const MAX_TOKEN_BYTES: usize = 64 * 1024;
#[derive(Clone, Copy)]
enum HelperKind {
    Shell,
    Packaged,
}
impl HelperKind {
    fn flag(self) -> &'static str {
        match self {
            Self::Shell => "--shell-launch-helper",
            Self::Packaged => "--packaged-activation-helper",
        }
    }
    fn result(self, code: u32) -> io::Result<()> {
        if code == 0 {
            Ok(())
        } else {
            Err(io::Error::other(match self {
                Self::Shell => Failure::Launch(code),
                Self::Packaged => Failure::Packaged(code as i32),
            }))
        }
    }
}
const WAIT_OBJECT_0: u32 = 0;
const INFINITE: u32 = u32::MAX;

#[repr(C)]
#[derive(Clone, Copy)]
struct SidAttributes {
    sid: *mut c_void,
    attributes: u32,
}
#[repr(C)]
struct StartupInfo {
    size: u32,
    reserved: *mut u16,
    desktop: *mut u16,
    title: *mut u16,
    x: u32,
    y: u32,
    x_size: u32,
    y_size: u32,
    x_chars: u32,
    y_chars: u32,
    fill: u32,
    flags: u32,
    show: u16,
    reserved_size: u16,
    reserved_bytes: *mut u8,
    input: Handle,
    output: Handle,
    error: Handle,
}
#[repr(C)]
struct StartupInfoEx {
    startup: StartupInfo,
    attributes: *mut c_void,
}
#[repr(C)]
struct ProcessInformation {
    process: Handle,
    thread: Handle,
    process_id: u32,
    thread_id: u32,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> Handle;
    fn GetCurrentProcessId() -> u32;
    fn GetProcessId(process: Handle) -> u32;
    fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> Handle;
    fn IsProcessInJob(process: Handle, job: Handle, result: *mut i32) -> i32;
    fn ProcessIdToSessionId(process_id: u32, session: *mut u32) -> i32;
    fn GetProcessHeap() -> Handle;
    fn HeapAlloc(heap: Handle, flags: u32, bytes: usize) -> *mut c_void;
    fn HeapFree(heap: Handle, flags: u32, block: *mut c_void) -> i32;
    fn InitializeProcThreadAttributeList(
        list: *mut c_void,
        count: u32,
        flags: u32,
        bytes: *mut usize,
    ) -> i32;
    fn UpdateProcThreadAttribute(
        list: *mut c_void,
        flags: u32,
        attribute: usize,
        value: *mut c_void,
        bytes: usize,
        previous: *mut c_void,
        returned: *mut usize,
    ) -> i32;
    fn DeleteProcThreadAttributeList(list: *mut c_void);
    fn CreateProcessW(
        application: *const u16,
        command: *mut u16,
        process_security: *const c_void,
        thread_security: *const c_void,
        inherit: i32,
        flags: u32,
        environment: *const c_void,
        directory: *const u16,
        startup: *const StartupInfo,
        information: *mut ProcessInformation,
    ) -> i32;
    fn TerminateProcess(process: Handle, code: u32) -> i32;
    fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
    fn GetExitCodeProcess(process: Handle, code: *mut u32) -> i32;
    fn GetEnvironmentStringsW() -> *mut u16;
    fn FreeEnvironmentStringsW(environment: *mut u16) -> i32;
    fn CompareStringOrdinal(
        left: *const u16,
        left_count: i32,
        right: *const u16,
        right_count: i32,
        ignore_case: i32,
    ) -> i32;
}
#[link(name = "user32")]
unsafe extern "system" {
    fn GetShellWindow() -> Handle;
    fn AllowSetForegroundWindow(process_id: u32) -> i32;
}
#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
    fn GetTokenInformation(
        token: Handle,
        class: u32,
        information: *mut c_void,
        bytes: u32,
        returned: *mut u32,
    ) -> i32;
    fn EqualSid(left: *const c_void, right: *const c_void) -> i32;
    fn IsValidSid(sid: *const c_void) -> i32;
    fn GetSidSubAuthorityCount(sid: *const c_void) -> *mut u8;
    fn GetSidSubAuthority(sid: *const c_void, index: u32) -> *mut u32;
}

struct OwnedHandle(Handle);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // This wrapper owns only real handles returned by Open/Create calls, never pseudo handles.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn checked(result: i32) -> io::Result<()> {
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn process_in_job(process: Handle) -> io::Result<bool> {
    let mut result = 0;
    // The process is borrowed for this call; NULL asks about membership in any job.
    checked(unsafe { IsProcessInJob(process, null_mut(), &mut result) })?;
    Ok(result != 0)
}
pub(super) fn in_job() -> io::Result<bool> {
    process_in_job(unsafe { GetCurrentProcess() })
}
fn session(process_id: u32) -> io::Result<u32> {
    let mut id = 0;
    checked(unsafe { ProcessIdToSessionId(process_id, &mut id) })?;
    Ok(id)
}
fn token(process: Handle) -> io::Result<OwnedHandle> {
    let mut handle = null_mut();
    checked(unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut handle) })?;
    Ok(OwnedHandle(handle))
}
struct TokenInformation(Vec<usize>);
impl TokenInformation {
    fn read(token: &OwnedHandle, class: u32) -> io::Result<Self> {
        let mut bytes = 0;
        // The size query is expected to fail with ERROR_INSUFFICIENT_BUFFER.
        let result = unsafe { GetTokenInformation(token.0, class, null_mut(), 0, &mut bytes) };
        if result != 0 {
            return Err(invalid("Unexpected process token size-query success"));
        }
        let code = unsafe { GetLastError() };
        if code != ERROR_INSUFFICIENT_BUFFER {
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        if bytes as usize > MAX_TOKEN_BYTES
            || (bytes as usize) < std::mem::size_of::<SidAttributes>()
        {
            return Err(invalid("Unexpected process token information size"));
        }
        // usize storage supplies the alignment required by TOKEN_USER/TOKEN_MANDATORY_LABEL.
        let mut data = vec![0usize; (bytes as usize).div_ceil(std::mem::size_of::<usize>())];
        let capacity = data.len() * std::mem::size_of::<usize>();
        checked(unsafe {
            GetTokenInformation(
                token.0,
                class,
                data.as_mut_ptr().cast(),
                capacity as u32,
                &mut bytes,
            )
        })?;
        if bytes as usize > capacity {
            return Err(invalid("Process token information grew unexpectedly"));
        }
        Ok(Self(data))
    }
    fn sid(&self) -> io::Result<*const c_void> {
        // OS-populated TOKEN_USER and TOKEN_MANDATORY_LABEL both start with SID_AND_ATTRIBUTES.
        // Validate the returned pointer before passing it to the SID routines; the buffer remains owned.
        let info = unsafe { self.0.as_ptr().cast::<SidAttributes>().read() };
        let start = self.0.as_ptr() as usize;
        let end = start + self.0.len() * std::mem::size_of::<usize>();
        let pointer = info.sid as usize;
        if pointer < start || pointer.checked_add(8).is_none_or(|value| value > end) {
            return Err(invalid("Invalid process token SID"));
        }
        // The subauthority count is in the already-bounded eight-byte SID header.
        // Bound the full SID before IsValidSid can read those subauthorities.
        let count = unsafe { *info.sid.cast::<u8>().add(1) } as usize;
        if pointer
            .checked_add(8 + count * 4)
            .is_none_or(|value| value > end)
            || unsafe { IsValidSid(info.sid) } == 0
        {
            return Err(invalid("Invalid process token SID"));
        }
        Ok(info.sid)
    }
    fn integrity(&self) -> io::Result<u32> {
        let sid = self.sid()?;
        let count = unsafe { *GetSidSubAuthorityCount(sid) };
        if count == 0 {
            return Err(invalid("Missing process integrity level"));
        }
        Ok(unsafe { *GetSidSubAuthority(sid, u32::from(count - 1)) })
    }
}
fn same_identity(
    process: Handle,
    process_id: u32,
    caller: &OwnedHandle,
    caller_session: u32,
    caller_integrity: u32,
) -> io::Result<()> {
    if session(process_id)? != caller_session {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Shell process is in another session",
        ));
    }
    let process_token = token(process)?;
    let user = TokenInformation::read(&process_token, TOKEN_USER)?;
    let caller_user = TokenInformation::read(caller, TOKEN_USER)?;
    if unsafe { EqualSid(user.sid()?, caller_user.sid()?) } == 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Shell process belongs to another user",
        ));
    }
    if TokenInformation::read(&process_token, TOKEN_INTEGRITY_LEVEL)?.integrity()?
        > caller_integrity
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Shell integrity exceeds the launcher",
        ));
    }
    Ok(())
}
fn shell_parent(
    caller: &OwnedHandle,
    caller_session: u32,
    caller_integrity: u32,
) -> io::Result<OwnedHandle> {
    let window = unsafe { GetShellWindow() };
    if window.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Desktop shell is unavailable",
        ));
    }
    let mut process_id = 0;
    if unsafe { GetWindowThreadProcessId(window, &mut process_id) } == 0 || process_id == 0 {
        return Err(io::Error::last_os_error());
    }
    let handle = unsafe {
        OpenProcess(
            PROCESS_CREATE_PROCESS | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            process_id,
        )
    };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    let process = OwnedHandle(handle);
    if unsafe { GetProcessId(process.0) } != process_id || process_in_job(process.0)? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Desktop shell cannot provide an independent launch",
        ));
    }
    same_identity(
        process.0,
        process_id,
        caller,
        caller_session,
        caller_integrity,
    )?;
    Ok(process)
}
struct Attributes<'a> {
    heap: Handle,
    pointer: *mut c_void,
    initialized: bool,
    _parent: PhantomData<&'a OwnedHandle>,
}
impl<'a> Attributes<'a> {
    fn new(parent: &'a OwnedHandle) -> io::Result<Self> {
        let mut bytes = 0;
        let result = unsafe { InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut bytes) };
        if result != 0
            || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER
            || bytes == 0
            || bytes > 64 * 1024
        {
            return Err(invalid("Unexpected process attribute size"));
        }
        let heap = unsafe { GetProcessHeap() };
        // HeapAlloc provides the native opaque attribute list's required alignment.
        let pointer = unsafe { HeapAlloc(heap, 0, bytes) };
        if pointer.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "Cannot allocate process attributes",
            ));
        }
        let mut attributes = Self {
            heap,
            pointer,
            initialized: false,
            _parent: PhantomData,
        };
        checked(unsafe { InitializeProcThreadAttributeList(pointer, 1, 0, &mut bytes) })?;
        attributes.initialized = true;
        // SDK: lpValue must remain valid until DeleteProcThreadAttributeList. PhantomData
        // keeps the borrowed, unmoved parent-handle slot alive until this owner's Drop.
        checked(unsafe {
            UpdateProcThreadAttribute(
                pointer,
                0,
                PARENT_PROCESS,
                (&parent.0 as *const Handle).cast_mut().cast(),
                std::mem::size_of::<Handle>(),
                null_mut(),
                null_mut(),
            )
        })?;
        Ok(attributes)
    }
}
impl Drop for Attributes<'_> {
    fn drop(&mut self) {
        unsafe {
            if self.initialized {
                DeleteProcThreadAttributeList(self.pointer);
            }
            HeapFree(self.heap, 0, self.pointer);
        }
    }
}
struct Environment(*mut u16);
impl Environment {
    fn snapshot() -> io::Result<Vec<u16>> {
        let pointer = unsafe { GetEnvironmentStringsW() };
        if pointer.is_null() {
            return Err(io::Error::last_os_error());
        }
        let environment = Self(pointer);
        // GetEnvironmentStringsW owns a complete double-NUL block. Retain its exact UTF-16,
        // including hidden =C: variables and unpaired units, until the bounded copy completes.
        for index in 1..MAX_ENVIRONMENT_UNITS {
            if unsafe { *pointer.add(index) == 0 && *pointer.add(index - 1) == 0 } {
                return Ok(
                    unsafe { std::slice::from_raw_parts(environment.0, index + 1) }.to_vec(),
                );
            }
        }
        Err(invalid("Process environment exceeds launch limits"))
    }
}
impl Drop for Environment {
    fn drop(&mut self) {
        unsafe {
            FreeEnvironmentStringsW(self.0);
        }
    }
}
fn is_path(entry: &[u16]) -> bool {
    entry.len() >= 5
        && entry[4] == b'=' as u16
        && entry[..4].iter().zip(b"PATH").all(|(actual, expected)| {
            *actual == u16::from(*expected) || *actual == u16::from(expected.to_ascii_lowercase())
        })
}
fn key_length(entry: &[u16]) -> io::Result<usize> {
    let start = usize::from(entry.first() == Some(&(b'=' as u16)));
    entry[start..]
        .iter()
        .position(|unit| *unit == b'=' as u16)
        .map(|position| start + position)
        .filter(|position| *position > start)
        .ok_or_else(|| invalid("Malformed process environment entry"))
}
fn environment_with_path(block: &[u16], prefix: Option<&str>) -> io::Result<Vec<u16>> {
    if !block.ends_with(&[0, 0]) || block.len() > MAX_ENVIRONMENT_UNITS {
        return Err(invalid("Malformed process environment block"));
    }
    let Some(prefix) = prefix else {
        return Ok(block.to_vec());
    };
    if prefix.is_empty() || prefix.as_bytes().contains(&0) {
        return Err(invalid("Invalid registered application PATH"));
    }
    let mut entries = Vec::new();
    let mut offset = 0;
    while offset + 1 < block.len() {
        let end = block[offset..]
            .iter()
            .position(|unit| *unit == 0)
            .ok_or_else(|| invalid("Malformed process environment block"))?
            + offset;
        if end == offset {
            if offset + 2 != block.len() {
                return Err(invalid("Malformed process environment block"));
            }
            break;
        }
        let entry = &block[offset..end];
        key_length(entry)?;
        entries.push(entry);
        offset = end + 1;
    }
    let mut replacement: Vec<_> = "PATH="
        .encode_utf16()
        .chain(prefix.encode_utf16())
        .collect();
    if let Some(existing) = entries
        .iter()
        .find(|entry| is_path(entry))
        .map(|entry| &entry[5..])
        .filter(|value| !value.is_empty())
    {
        replacement.push(b';' as u16);
        replacement.extend_from_slice(existing);
    }
    if replacement.len() - 5 >= MAX_COMMAND_UNITS {
        return Err(invalid(
            "Registered application PATH exceeds the Windows limit",
        ));
    }
    let mut result = Vec::with_capacity(block.len() + replacement.len() + 1);
    let mut inserted = false;
    for entry in entries {
        if is_path(entry) {
            if !inserted {
                result.extend_from_slice(&replacement);
                result.push(0);
                inserted = true;
            }
            continue;
        }
        if !inserted {
            let size = key_length(entry)?;
            let compare = unsafe {
                CompareStringOrdinal(entry.as_ptr(), size as i32, replacement.as_ptr(), 4, 1)
            };
            if compare == 0 {
                return Err(io::Error::last_os_error());
            }
            if compare == 3 {
                result.extend_from_slice(&replacement);
                result.push(0);
                inserted = true;
            }
        }
        result.extend_from_slice(entry);
        result.push(0);
    }
    if !inserted {
        result.extend_from_slice(&replacement);
        result.push(0);
    }
    result.push(0);
    if result.len() > MAX_ENVIRONMENT_UNITS {
        return Err(invalid("Process environment exceeds launch limits"));
    }
    Ok(result)
}
fn quoted_argument(argument: &OsStr, output: &mut Vec<u16>) -> io::Result<()> {
    if argument.encode_wide().take(MAX_COMMAND_UNITS).count() == MAX_COMMAND_UNITS {
        return Err(invalid("Launch argument exceeds the Windows limit"));
    }
    output.push(b'"' as u16);
    let mut slashes = 0;
    for unit in argument.encode_wide() {
        if unit == 0 {
            return Err(invalid("Launch argument contains NUL"));
        }
        if unit == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        output.extend(std::iter::repeat_n(
            b'\\' as u16,
            if unit == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        slashes = 0;
        output.push(unit);
    }
    output.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    output.push(b'"' as u16);
    Ok(())
}
fn command_line(executable: &Path, argument: &OsStr, kind: HelperKind) -> io::Result<Vec<u16>> {
    let mut command = Vec::new();
    quoted_argument(executable.as_os_str(), &mut command)?;
    command.push(b' ' as u16);
    command.extend(kind.flag().encode_utf16());
    command.push(b' ' as u16);
    quoted_argument(argument, &mut command)?;
    command.push(0);
    if command.len() > MAX_COMMAND_UNITS {
        return Err(invalid("Launch command exceeds the Windows limit"));
    }
    Ok(command)
}
struct Child {
    process: OwnedHandle,
    _thread: OwnedHandle,
    abort: bool,
}
impl Drop for Child {
    fn drop(&mut self) {
        // Until completion is confirmed this is only our owned helper. Failed
        // post-creation checks and wait errors reclaim it before releasing handles.
        if self.abort && unsafe { TerminateProcess(self.process.0, 1) } != 0 {
            unsafe {
                WaitForSingleObject(self.process.0, INFINITE);
            }
        }
    }
}
pub(super) fn detached_helper(path: &Path, registered_path: Option<&str>) -> io::Result<()> {
    execute_helper(path.as_os_str(), HelperKind::Shell, registered_path)
}
pub(super) fn detached_activation(id: &str) -> io::Result<()> {
    let valid = !id.contains('\0')
        && id.encode_utf16().count() <= 129
        && id.split_once('!').is_some_and(|(family, application)| {
            !family.is_empty() && !application.is_empty() && !application.contains('!')
        });
    if !valid {
        return Err(io::Error::other(Failure::Packaged(0x80070057u32 as i32)));
    }
    execute_helper(OsStr::new(id), HelperKind::Packaged, None)
}
fn execute_helper(
    argument: &OsStr,
    kind: HelperKind,
    registered_path: Option<&str>,
) -> io::Result<()> {
    let executable = std::env::current_exe()?;
    let mut application: Vec<_> = executable.as_os_str().encode_wide().collect();
    if application.contains(&0) {
        return Err(invalid("Helper executable contains NUL"));
    }
    application.push(0);
    let mut command = command_line(&executable, argument, kind)?;
    let environment = environment_with_path(&Environment::snapshot()?, registered_path)?;
    let caller = token(unsafe { GetCurrentProcess() })?;
    let caller_session = session(unsafe { GetCurrentProcessId() })?;
    let caller_integrity = TokenInformation::read(&caller, TOKEN_INTEGRITY_LEVEL)?.integrity()?;
    let parent = shell_parent(&caller, caller_session, caller_integrity)?;
    let attributes = Attributes::new(&parent)?;
    // All fields are integer scalars or raw pointers, for which zero is a valid value.
    let mut startup: StartupInfoEx = unsafe { std::mem::zeroed() };
    startup.startup.size = std::mem::size_of::<StartupInfoEx>() as u32;
    startup.attributes = attributes.pointer;
    let mut information: ProcessInformation = unsafe { std::mem::zeroed() };
    // Exact lpApplicationName avoids executable-name search. The mutable command, explicit
    // current environment, native attributes and parent-handle slot outlive this call.
    // Parent attributes inherit the verified shell's token and may lower caller integrity.
    checked(unsafe {
        CreateProcessW(
            application.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            0,
            CREATE_FLAGS,
            environment.as_ptr().cast(),
            null(),
            &startup.startup,
            &mut information,
        )
    })?;
    let mut child = Child {
        process: OwnedHandle(information.process),
        _thread: OwnedHandle(information.thread),
        abort: true,
    };
    // The helper starts immediately and checks its own job before any Shell call.
    // This avoids leaving a suspended process if the terminal terminates the caller.
    unsafe {
        AllowSetForegroundWindow(information.process_id);
    }
    let verified = (|| {
        if process_in_job(child.process.0)? {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Helper remained coupled to a process job",
            ));
        }
        same_identity(
            child.process.0,
            information.process_id,
            &caller,
            caller_session,
            caller_integrity,
        )
    })();
    if let Err(error) = verified {
        // An immediately completed helper may have no remaining token/PID to query.
        // The shell's identity was already verified before creation, and the trusted
        // helper independently refuses any job before activating the original path.
        // Keep actual identity/job policy rejections fatal; only a native query failure
        // on an already-signaled process can defer to that helper's completed result.
        if error.raw_os_error().is_some()
            && unsafe { WaitForSingleObject(child.process.0, 0) } == WAIT_OBJECT_0
        {
            child.abort = false;
            return helper_result(child.process.0, kind);
        }
        return Err(error);
    }
    if unsafe { WaitForSingleObject(child.process.0, INFINITE) } != WAIT_OBJECT_0 {
        return Err(io::Error::last_os_error());
    }
    child.abort = false;
    helper_result(child.process.0, kind)
}
fn helper_result(process: Handle, kind: HelperKind) -> io::Result<()> {
    let mut code = 0;
    checked(unsafe { GetExitCodeProcess(process, &mut code) })?;
    kind.result(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn CommandLineToArgvW(command: *const u16, count: *mut i32) -> *mut *mut u16;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }
    struct Arguments(*mut *mut u16);
    impl Drop for Arguments {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0.cast());
            }
        }
    }
    fn parse_arguments(command: &[u16]) -> Vec<OsString> {
        assert_eq!(command.last(), Some(&0));
        let mut count = 0;
        let arguments = Arguments(unsafe { CommandLineToArgvW(command.as_ptr(), &mut count) });
        assert!(!arguments.0.is_null());
        (0..count as usize)
            .map(|index| {
                let pointer = unsafe { *arguments.0.add(index) };
                let mut length = 0;
                while unsafe { *pointer.add(length) } != 0 {
                    length += 1;
                }
                OsString::from_wide(unsafe { std::slice::from_raw_parts(pointer, length) })
            })
            .collect()
    }
    fn block(entries: &[&str]) -> Vec<u16> {
        let mut result = Vec::new();
        for entry in entries {
            result.extend(entry.encode_utf16());
            result.push(0);
        }
        if entries.is_empty() {
            result.push(0);
        }
        result.push(0);
        result
    }
    fn entries(block: &[u16]) -> Vec<&[u16]> {
        block
            .split(|unit| *unit == 0)
            .filter(|value| !value.is_empty())
            .collect()
    }

    #[test]
    fn quoting_roundtrips_through_the_windows_command_line_parser() {
        let mut cases: Vec<OsString> = [
            "",
            "simple",
            "space and 中文",
            "tab\tseparated",
            r"C:\trailing\",
            r#"embedded"quote"#,
            r#"slashes\\\"quote"#,
            r"\\server\中文 dir\app.lnk",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        cases.push(OsString::from_wide(&[65, 0xd800, 0x5c]));
        for argument in cases {
            let mut command: Vec<_> = "helper.exe ".encode_utf16().collect();
            quoted_argument(&argument, &mut command).unwrap();
            command.push(0);
            assert_eq!(
                parse_arguments(&command),
                [OsString::from("helper.exe"), argument]
            );
        }
    }

    #[test]
    fn helper_command_keeps_original_relative_or_absolute_target_as_one_argument() {
        let executable = Path::new(r"C:\Program Files\PicoRun\picorun.exe");
        for target in [
            Path::new(r"C:\应用 with spaces\target.lnk"),
            Path::new(r"relative 目录\reference.appref-ms"),
        ] {
            let command = command_line(executable, target.as_os_str(), HelperKind::Shell).unwrap();
            assert_eq!(
                parse_arguments(&command),
                [
                    executable.as_os_str(),
                    OsStr::new("--shell-launch-helper"),
                    target.as_os_str()
                ]
            );
        }
        let bad = Path::new(OsStr::new("bad\0path.lnk"));
        assert!(command_line(executable, bad.as_os_str(), HelperKind::Shell).is_err());
        let large = OsString::from("x".repeat(MAX_COMMAND_UNITS));
        assert!(command_line(executable, &large, HelperKind::Shell).is_err());
    }

    #[test]
    fn private_environment_preserves_hidden_drive_variables_marker_and_raw_utf16() {
        let mut original = block(&[
            "=C:=C:\\Working",
            "MARKER=参数 with spaces",
            "OPAQUE=placeholder",
            "Path=C:\\Existing",
        ]);
        let placeholder: Vec<_> = "placeholder".encode_utf16().collect();
        let position = original
            .windows(placeholder.len())
            .position(|value| value == placeholder)
            .unwrap();
        original[position] = 0xd800;
        let modified = environment_with_path(&original, Some("D:\\New Path;E:\\支持")).unwrap();
        let original_entries = entries(&original);
        let modified_entries = entries(&modified);
        for original_entry in original_entries.iter().filter(|entry| !is_path(entry)) {
            assert!(modified_entries.contains(original_entry));
        }
        let path = modified_entries
            .iter()
            .find(|entry| is_path(entry))
            .unwrap();
        assert_eq!(
            String::from_utf16(path).unwrap(),
            "PATH=D:\\New Path;E:\\支持;C:\\Existing"
        );
        assert_eq!(environment_with_path(&original, None).unwrap(), original);
    }

    #[test]
    fn private_environment_inserts_missing_path_in_native_name_order() {
        let original = block(&["=C:=C:\\Working", "A=first", "ZZZ=last"]);
        let modified = environment_with_path(&original, Some("C:\\Private")).unwrap();
        assert_eq!(
            modified,
            block(&["=C:=C:\\Working", "A=first", "PATH=C:\\Private", "ZZZ=last"])
        );
        assert_eq!(
            environment_with_path(&[0, 0], Some("C:\\Private")).unwrap(),
            block(&["PATH=C:\\Private"])
        );
    }

    #[test]
    fn private_environment_deduplicates_path_and_bounds_the_combined_value() {
        let duplicate = block(&["PATH=C:\\First", "Path=C:\\Second", "ZZZ=last"]);
        let modified = environment_with_path(&duplicate, Some("D:\\Prefix")).unwrap();
        let modified = entries(&modified);
        assert_eq!(modified.iter().filter(|entry| is_path(entry)).count(), 1);
        assert_eq!(
            String::from_utf16(modified[0]).unwrap(),
            "PATH=D:\\Prefix;C:\\First"
        );
        let prefix = "x".repeat(MAX_COMMAND_UNITS - 1);
        assert!(environment_with_path(&[0, 0], Some(&prefix)).is_ok());
        assert!(environment_with_path(&block(&["PATH=old"]), Some(&prefix)).is_err());
        assert!(environment_with_path(&[0, 0], Some(&"x".repeat(MAX_COMMAND_UNITS))).is_err());
        for invalid_prefix in ["", "nul\0suffix"] {
            assert!(environment_with_path(&[0, 0], Some(invalid_prefix)).is_err());
        }
        for invalid_block in [vec![0], vec![0, 0, 0], block(&["BAD_ENTRY"])] {
            assert!(environment_with_path(&invalid_block, Some("valid")).is_err());
        }
    }

    #[test]
    fn startup_structures_match_the_windows_sdk_layout() {
        let (startup, extended, process, sid) = if cfg!(target_pointer_width = "64") {
            (104, 112, 24, 16)
        } else {
            (68, 72, 16, 8)
        };
        assert_eq!(std::mem::size_of::<StartupInfo>(), startup);
        assert_eq!(std::mem::size_of::<StartupInfoEx>(), extended);
        assert_eq!(std::mem::offset_of!(StartupInfoEx, attributes), startup);
        assert_eq!(std::mem::size_of::<ProcessInformation>(), process);
        assert_eq!(std::mem::size_of::<SidAttributes>(), sid);
    }

    #[test]
    fn current_token_and_native_parent_attributes_are_readable_without_extra_privileges() {
        let current = unsafe { GetCurrentProcess() };
        let process_id = unsafe { GetCurrentProcessId() };
        let token = token(current).unwrap();
        let integrity = TokenInformation::read(&token, TOKEN_INTEGRITY_LEVEL)
            .unwrap()
            .integrity()
            .unwrap();
        same_identity(
            current,
            process_id,
            &token,
            session(process_id).unwrap(),
            integrity,
        )
        .unwrap();
        let handle = unsafe {
            OpenProcess(
                PROCESS_CREATE_PROCESS | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                process_id,
            )
        };
        assert!(!handle.is_null());
        let parent = OwnedHandle(handle);
        let attributes = Attributes::new(&parent).unwrap();
        assert!(!attributes.pointer.is_null());
        let environment = Environment::snapshot().unwrap();
        assert!(environment.ends_with(&[0, 0]));
    }

    #[test]
    fn packaged_command_preserves_aumid_and_hresult_without_shell_path_conversion() {
        let executable = Path::new(r"C:\Program Files\PicoRun\picorun.exe");
        let id = "Synthetic.商店_1234567890abc!Application";
        let command = command_line(executable, OsStr::new(id), HelperKind::Packaged).unwrap();
        assert_eq!(
            parse_arguments(&command),
            [
                executable.as_os_str(),
                OsStr::new("--packaged-activation-helper"),
                OsStr::new(id)
            ]
        );
        let code = 0x80070002u32;
        let error = HelperKind::Packaged.result(code).unwrap_err();
        assert!(
            matches!(error.get_ref().and_then(|cause| cause.downcast_ref::<Failure>()), Some(Failure::Packaged(actual)) if *actual == code as i32)
        );
        let error = HelperKind::Shell.result(code).unwrap_err();
        assert!(
            matches!(error.get_ref().and_then(|cause| cause.downcast_ref::<Failure>()), Some(Failure::Launch(actual)) if *actual == code)
        );
        assert!(HelperKind::Shell.result(0).is_ok());
        assert!(HelperKind::Packaged.result(0).is_ok());
    }

    #[test]
    fn malformed_activation_id_is_rejected_before_creating_any_process() {
        for id in [
            "",
            "Ordinary.Desktop",
            "!App",
            "Family!",
            "Family!App!Extra",
            "Family!App\0",
        ] {
            let error = detached_activation(id).unwrap_err();
            assert!(
                matches!(error.get_ref().and_then(|cause| cause.downcast_ref::<Failure>()), Some(Failure::Packaged(actual)) if *actual == 0x80070057u32 as i32)
            );
        }
        assert!(detached_activation(&format!("Family!{}", "a".repeat(123))).is_err());
    }
}
