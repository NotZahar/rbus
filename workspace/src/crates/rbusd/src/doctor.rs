use std::fs;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::AtomicU32;

use rbus_core::constants::{
    CONTROL_PROTOCOL_VERSION, MAX_SEGMENT_SIZE, RBUS_API_VERSION, STREAM_SEGMENT_ABI_VERSION,
    TOPIC_WAIT_ABI_VERSION,
};
use rbus_core::layout::calculate_segment_layout;
use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::eventfd::create_eventfd;
use rbus_ipc::fd::OwnedFileDescriptor;
use rbus_ipc::futex::{futex_wake_private, futex_wake_shared};
use rbus_ipc::memfd::{create_memfd, seal_file_descriptor_size};
use rbus_ipc::mmap::MappedRegion;
use rbus_ipc::scm_rights::{receive_file_descriptors, send_file_descriptors};
use rbus_ipc::unix_socket::runtime_directory_path;

use crate::domain::domain_socket_path;
use crate::registry::DaemonRegistry;
use crate::warnings::warning_report;

const COMPONENT_RBUSD: &str = "rbusd";
const DOCTOR_MEMFD_SIZE: usize = 4096;
const DOCTOR_SLOT_SIZE: usize = 64;
const DOCTOR_DEPTH: usize = 2;
const FUTEX_WAKE_COUNT: u32 = 1;
const DIRECTORY_PERMISSION_MASK: u32 = 0o777;
const EXPECTED_DIRECTORY_MODE: u32 = 0o700;
const DOCTOR_MAPPING_SENTINEL: u8 = 0xa5;
const DOCTOR_SCM_PAYLOAD: [u8; 4] = *b"rbus";
const DOCTOR_SCM_PAYLOAD_SIZE: usize = DOCTOR_SCM_PAYLOAD.len();
const DOCTOR_SCM_FILE_DESCRIPTOR_COUNT: usize = 1;

/// Real doctor checks for a daemon domain
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DoctorReport {
    /// Canonical runtime directory path
    pub runtime_directory_path: String,
    /// Runtime directory exists
    pub runtime_directory_exists: bool,
    /// Runtime directory owner UID
    pub runtime_directory_owner_uid: u32,
    /// Runtime directory permission bits
    pub runtime_directory_mode: u32,
    /// Socket pathname
    pub socket_path: String,
    /// Socket file exists
    pub socket_exists: bool,
    /// Socket directory exists
    pub socket_directory_exists: bool,
    /// Socket directory owner UID
    pub socket_directory_owner_uid: u32,
    /// Socket directory permission bits
    pub socket_directory_mode: u32,
    /// Public API version
    pub api_version: &'static str,
    /// Control protocol version
    pub control_protocol_version: u16,
    /// Stream segment ABI version
    pub stream_segment_abi_version: u32,
    /// Topic wait segment ABI version
    pub topic_wait_abi_version: u32,
    /// memfd_create/ftruncate/seal works
    pub memfd_supported: bool,
    /// mmap MAP_SHARED works
    pub mmap_supported: bool,
    /// eventfd works
    pub eventfd_supported: bool,
    /// futex syscall works
    pub futex_supported: bool,
    /// Unix socket SCM_RIGHTS FD passing works
    pub scm_rights_supported: bool,
    /// Compile-time maximum stream segment size
    pub max_segment_size: u64,
    /// RLIMIT_NOFILE soft
    pub nofile_soft: u64,
    /// RLIMIT_NOFILE hard
    pub nofile_hard: u64,
    /// RLIMIT_MEMLOCK soft
    pub memlock_soft: u64,
    /// RLIMIT_MEMLOCK hard
    pub memlock_hard: u64,
    /// RLIMIT_FSIZE soft
    pub file_size_soft: u64,
    /// RLIMIT_FSIZE hard
    pub file_size_hard: u64,
    /// RLIMIT_AS soft
    pub address_space_soft: u64,
    /// RLIMIT_AS hard
    pub address_space_hard: u64,
    /// Current warning counter
    pub warnings_total: u64,
}

/// Daemon runtime metadata
pub fn doctor_domain(registry: &DaemonRegistry, domain: &str) -> Result<DoctorReport, Warning> {
    if domain.is_empty() {
        return Err(Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_RBUSD,
            "doctor_domain",
        ));
    }
    let expected_owner_uid = effective_user_id();
    let runtime_directory = runtime_directory_path();
    let runtime_directory_status = check_owned_directory(
        &runtime_directory,
        expected_owner_uid,
        "doctor_domain::runtime_directory",
    )?;
    let socket_path = domain_socket_path(domain);
    check_socket_path(&socket_path)?;
    let socket_directory = socket_path.parent().ok_or_else(|| {
        Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_RBUSD,
            "doctor_domain::socket_parent",
        )
    })?;
    let socket_directory_status = check_owned_directory(
        socket_directory,
        expected_owner_uid,
        "doctor_domain::socket_directory",
    )?;
    check_abi_layout()?;
    check_shared_memory_primitives()?;
    let _eventfd = create_eventfd()?;
    check_futex()?;
    let nofile = read_resource_limit(libc::RLIMIT_NOFILE, "doctor_domain::nofile")?;
    let memlock = read_resource_limit(libc::RLIMIT_MEMLOCK, "doctor_domain::memlock")?;
    let file_size = read_resource_limit(libc::RLIMIT_FSIZE, "doctor_domain::file_size")?;
    let address_space = read_resource_limit(libc::RLIMIT_AS, "doctor_domain::address_space")?;
    check_max_segment_size(file_size, address_space)?;
    let warnings = warning_report(registry)?;

    Ok(DoctorReport {
        runtime_directory_path: runtime_directory.display().to_string(),
        runtime_directory_exists: runtime_directory_status.exists,
        runtime_directory_owner_uid: runtime_directory_status.owner_uid,
        runtime_directory_mode: runtime_directory_status.mode,
        socket_path: socket_path.display().to_string(),
        socket_exists: true,
        socket_directory_exists: socket_directory_status.exists,
        socket_directory_owner_uid: socket_directory_status.owner_uid,
        socket_directory_mode: socket_directory_status.mode,
        api_version: RBUS_API_VERSION,
        control_protocol_version: CONTROL_PROTOCOL_VERSION,
        stream_segment_abi_version: STREAM_SEGMENT_ABI_VERSION,
        topic_wait_abi_version: TOPIC_WAIT_ABI_VERSION,
        memfd_supported: true,
        mmap_supported: true,
        eventfd_supported: true,
        futex_supported: true,
        scm_rights_supported: true,
        max_segment_size: MAX_SEGMENT_SIZE as u64,
        nofile_soft: nofile.soft,
        nofile_hard: nofile.hard,
        memlock_soft: memlock.soft,
        memlock_hard: memlock.hard,
        file_size_soft: file_size.soft,
        file_size_hard: file_size.hard,
        address_space_soft: address_space.soft,
        address_space_hard: address_space.hard,
        warnings_total: warnings.warnings_total,
    })
}

/// CLI metadata
pub fn format_doctor_report(domain: &str, report: &DoctorReport, json: bool) -> String {
    if json {
        format!(
            concat!(
                "{{\"domain\":\"{}\",\"runtime_directory_path\":\"{}\",",
                "\"runtime_directory_exists\":{},\"runtime_directory_owner_uid\":{},",
                "\"runtime_directory_mode\":{},\"socket_path\":\"{}\",",
                "\"socket_exists\":{},\"socket_directory_exists\":{},",
                "\"socket_directory_owner_uid\":{},\"socket_directory_mode\":{},",
                "\"api_version\":\"{}\",\"control_protocol_version\":{},",
                "\"stream_segment_abi_version\":{},\"topic_wait_abi_version\":{},",
                "\"memfd\":{},\"mmap\":{},\"eventfd\":{},\"futex\":{},\"scm_rights\":{},",
                "\"max_segment_size\":{},\"nofile_soft\":{},\"nofile_hard\":{},",
                "\"memlock_soft\":{},\"memlock_hard\":{},\"file_size_soft\":{},",
                "\"file_size_hard\":{},\"address_space_soft\":{},\"address_space_hard\":{},",
                "\"warnings_total\":{}}}"
            ),
            escape_json_string(domain),
            escape_json_string(&report.runtime_directory_path),
            report.runtime_directory_exists,
            report.runtime_directory_owner_uid,
            report.runtime_directory_mode,
            escape_json_string(&report.socket_path),
            report.socket_exists,
            report.socket_directory_exists,
            report.socket_directory_owner_uid,
            report.socket_directory_mode,
            escape_json_string(report.api_version),
            report.control_protocol_version,
            report.stream_segment_abi_version,
            report.topic_wait_abi_version,
            report.memfd_supported,
            report.mmap_supported,
            report.eventfd_supported,
            report.futex_supported,
            report.scm_rights_supported,
            report.max_segment_size,
            report.nofile_soft,
            report.nofile_hard,
            report.memlock_soft,
            report.memlock_hard,
            report.file_size_soft,
            report.file_size_hard,
            report.address_space_soft,
            report.address_space_hard,
            report.warnings_total
        )
    } else {
        format!(
            concat!(
                "domain {}: runtime_directory_path={} runtime_directory_exists={} ",
                "runtime_directory_owner_uid={} runtime_directory_mode={:o} ",
                "socket_path={} socket_exists={} socket_directory_exists={} ",
                "socket_directory_owner_uid={} socket_directory_mode={:o} ",
                "api_version={} control_protocol_version={} stream_segment_abi_version={} ",
                "topic_wait_abi_version={} memfd={} mmap={} eventfd={} futex={} scm_rights={} ",
                "max_segment_size={} nofile_soft={} nofile_hard={} memlock_soft={} ",
                "memlock_hard={} file_size_soft={} file_size_hard={} address_space_soft={} ",
                "address_space_hard={} ",
                "warnings_total={}"
            ),
            domain,
            report.runtime_directory_path,
            report.runtime_directory_exists,
            report.runtime_directory_owner_uid,
            report.runtime_directory_mode,
            report.socket_path,
            report.socket_exists,
            report.socket_directory_exists,
            report.socket_directory_owner_uid,
            report.socket_directory_mode,
            report.api_version,
            report.control_protocol_version,
            report.stream_segment_abi_version,
            report.topic_wait_abi_version,
            report.memfd_supported,
            report.mmap_supported,
            report.eventfd_supported,
            report.futex_supported,
            report.scm_rights_supported,
            report.max_segment_size,
            report.nofile_soft,
            report.nofile_hard,
            report.memlock_soft,
            report.memlock_hard,
            report.file_size_soft,
            report.file_size_hard,
            report.address_space_soft,
            report.address_space_hard,
            report.warnings_total
        )
    }
}

fn check_abi_layout() -> Result<(), Warning> {
    let _layout = calculate_segment_layout(DOCTOR_SLOT_SIZE, DOCTOR_DEPTH)?;
    Ok(())
}

fn check_shared_memory_primitives() -> Result<(), Warning> {
    let file_descriptor = create_memfd("rbus-doctor", DOCTOR_MEMFD_SIZE)?;
    seal_file_descriptor_size(&file_descriptor)?;
    let mut mapping = MappedRegion::map_shared(
        file_descriptor.as_raw_file_descriptor(),
        DOCTOR_MEMFD_SIZE,
        true,
    )?;
    {
        let mapping_bytes = mapping.as_mutable_slice();
        mapping_bytes[0] = DOCTOR_MAPPING_SENTINEL;
    }
    if mapping.as_slice()[0] != DOCTOR_MAPPING_SENTINEL {
        return Err(Warning::new(
            WarningCode::InternalInvariantWarning,
            COMPONENT_RBUSD,
            "check_shared_memory_primitives::mmap_roundtrip",
        ));
    }
    check_scm_rights(&file_descriptor)
}

fn check_scm_rights(file_descriptor: &OwnedFileDescriptor) -> Result<(), Warning> {
    let (sender, receiver) = UnixStream::pair().map_err(|error| {
        warning_from_io(
            error,
            WarningCode::SyscallFailed,
            "check_scm_rights::socket_pair",
        )
    })?;
    let sent_bytes = send_file_descriptors(
        sender.as_raw_fd(),
        &DOCTOR_SCM_PAYLOAD,
        &[file_descriptor.as_raw_file_descriptor()],
    )?;
    let mut payload_buffer = [0_u8; DOCTOR_SCM_PAYLOAD_SIZE];
    let received = receive_file_descriptors(
        receiver.as_raw_fd(),
        &mut payload_buffer,
        DOCTOR_SCM_FILE_DESCRIPTOR_COUNT,
    )?;
    let valid = sent_bytes == DOCTOR_SCM_PAYLOAD_SIZE
        && received.byte_count == DOCTOR_SCM_PAYLOAD_SIZE
        && payload_buffer == DOCTOR_SCM_PAYLOAD
        && received.file_descriptors.len() == DOCTOR_SCM_FILE_DESCRIPTOR_COUNT;
    if valid {
        Ok(())
    } else {
        Err(Warning::new(
            WarningCode::InternalInvariantWarning,
            COMPONENT_RBUSD,
            "check_scm_rights::roundtrip",
        ))
    }
}

fn check_futex() -> Result<(), Warning> {
    let futex_word = AtomicU32::new(0);
    let _private_woken = futex_wake_private(&futex_word, FUTEX_WAKE_COUNT)?;
    let _shared_woken = futex_wake_shared(&futex_word, FUTEX_WAKE_COUNT)?;
    Ok(())
}

fn check_socket_path(socket_path: &Path) -> Result<(), Warning> {
    let socket_metadata = fs::symlink_metadata(socket_path).map_err(|error| {
        warning_from_io(
            error,
            WarningCode::DaemonUnavailable,
            "doctor_domain::socket",
        )
    })?;
    if socket_metadata.file_type().is_socket() {
        Ok(())
    } else {
        Err(Warning::new(
            WarningCode::DaemonUnavailable,
            COMPONENT_RBUSD,
            "doctor_domain::socket_type",
        ))
    }
}

fn check_owned_directory(
    directory_path: &Path,
    expected_owner_uid: u32,
    operation: &'static str,
) -> Result<DirectoryStatus, Warning> {
    let metadata = fs::symlink_metadata(directory_path)
        .map_err(|error| warning_from_io(error, WarningCode::PermissionDenied, operation))?;
    let mode = metadata.permissions().mode() & DIRECTORY_PERMISSION_MASK;
    let owner_uid = metadata.uid();
    let valid = metadata.file_type().is_dir()
        && owner_uid == expected_owner_uid
        && mode == EXPECTED_DIRECTORY_MODE;
    if valid {
        Ok(DirectoryStatus {
            exists: true,
            owner_uid,
            mode,
        })
    } else {
        Err(Warning::new(
            WarningCode::PermissionDenied,
            COMPONENT_RBUSD,
            operation,
        ))
    }
}

fn check_max_segment_size(
    file_size: ResourceLimit,
    address_space: ResourceLimit,
) -> Result<(), Warning> {
    let _max_segment_size = libc::off_t::try_from(MAX_SEGMENT_SIZE).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_RBUSD,
            "check_max_segment_size::off_t",
        )
    })?;
    ensure_resource_limit_allows(
        file_size.soft,
        MAX_SEGMENT_SIZE,
        "check_max_segment_size::file_size",
    )?;
    ensure_resource_limit_allows(
        address_space.soft,
        MAX_SEGMENT_SIZE,
        "check_max_segment_size::address_space",
    )?;
    if calculate_segment_layout(MAX_SEGMENT_SIZE, DOCTOR_DEPTH).is_ok() {
        return Err(Warning::new(
            WarningCode::InternalInvariantWarning,
            COMPONENT_RBUSD,
            "check_max_segment_size::layout_limit",
        ));
    }
    Ok(())
}

fn ensure_resource_limit_allows(
    limit: u64,
    required_size: usize,
    operation: &'static str,
) -> Result<(), Warning> {
    let required_size = u64::try_from(required_size).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_RBUSD,
            "ensure_resource_limit_allows::required_size",
        )
    })?;
    if limit != libc::RLIM_INFINITY && limit < required_size {
        return Err(Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_RBUSD,
            operation,
        ));
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct DirectoryStatus {
    exists: bool,
    owner_uid: u32,
    mode: u32,
}

#[derive(Clone, Copy)]
struct ResourceLimit {
    soft: u64,
    hard: u64,
}

fn read_resource_limit(
    resource: libc::__rlimit_resource_t,
    operation: &'static str,
) -> Result<ResourceLimit, Warning> {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: limit points to a valid stack allocation for getrlimit to fill
    let result = unsafe { libc::getrlimit(resource, &mut limit) };
    if result != 0 {
        return Err(Warning::new(
            WarningCode::SyscallFailed,
            COMPONENT_RBUSD,
            operation,
        ));
    }
    Ok(ResourceLimit {
        soft: limit.rlim_cur,
        hard: limit.rlim_max,
    })
}

fn effective_user_id() -> u32 {
    // SAFETY: Required invariants are upheld
    unsafe { libc::geteuid() }
}

fn warning_from_io(
    error: std::io::Error,
    warning_code: WarningCode,
    operation: &'static str,
) -> Warning {
    match error.raw_os_error() {
        Some(errno) => Warning::new(warning_code, COMPONENT_RBUSD, operation).with_os_errno(errno),
        None => Warning::new(warning_code, COMPONENT_RBUSD, operation),
    }
}

fn escape_json_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
