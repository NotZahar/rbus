use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const SERVICE_UNIT_NAME: &str = "rbusd@.service";
const SOCKET_UNIT_NAME: &str = "rbusd.socket";
const USER_UNIT_DIRECTORY: &str = "systemd/user";
const UNIT_FILE_MODE_MASK: u32 = 0o777;
const EXPECTED_UNIT_FILE_MODE: u32 = 0o644;
const SERVICE_EXEC_START: &str = "ExecStart=%h/.cargo/bin/rbusd --domain %i";
const SERVICE_INSTALL_TARGET: &str = "WantedBy=default.target";
const SOCKET_LISTEN_STREAM: &str = "ListenStream=%t/rbus/default.sock";
const SOCKET_MODE: &str = "SocketMode=0600";
const SOCKET_INSTALL_TARGET: &str = "WantedBy=sockets.target";

struct UnitSources {
    service: PathBuf,
    socket: PathBuf,
}

#[test]
fn systemd_user_unit_install_check_copies_required_units() {
    let source_directory = systemd_source_directory();
    let install_root = unique_temp_path("install-ok");
    let user_unit_directory = install_root.join(USER_UNIT_DIRECTORY);
    let sources = UnitSources {
        service: source_directory.join(SERVICE_UNIT_NAME),
        socket: source_directory.join(SOCKET_UNIT_NAME),
    };

    install_user_units(&sources, &user_unit_directory).expect("unit install should succeed");

    let installed_service = user_unit_directory.join(SERVICE_UNIT_NAME);
    let installed_socket = user_unit_directory.join(SOCKET_UNIT_NAME);
    assert_installed_unit(&sources.service, &installed_service);
    assert_installed_unit(&sources.socket, &installed_socket);

    let service_contents =
        fs::read_to_string(&installed_service).expect("service unit should be readable");
    let socket_contents =
        fs::read_to_string(&installed_socket).expect("socket unit should be readable");
    assert!(service_contents.contains(SERVICE_EXEC_START));
    assert!(service_contents.contains(SERVICE_INSTALL_TARGET));
    assert!(socket_contents.contains(SOCKET_LISTEN_STREAM));
    assert!(socket_contents.contains(SOCKET_MODE));
    assert!(socket_contents.contains(SOCKET_INSTALL_TARGET));

    fs::remove_dir_all(install_root).expect("test install root should be removed");
}

#[test]
fn systemd_user_unit_install_check_rejects_missing_unit_source() {
    let source_directory = systemd_source_directory();
    let install_root = unique_temp_path("install-missing");
    let user_unit_directory = install_root.join(USER_UNIT_DIRECTORY);
    let sources = UnitSources {
        service: source_directory.join("missing.service"),
        socket: source_directory.join(SOCKET_UNIT_NAME),
    };

    let error = install_user_units(&sources, &user_unit_directory)
        .expect_err("unit install should fail for a missing source");

    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    let _ = fs::remove_dir_all(install_root);
}

fn install_user_units(sources: &UnitSources, user_unit_directory: &Path) -> io::Result<()> {
    fs::create_dir_all(user_unit_directory)?;
    copy_unit(
        &sources.service,
        &user_unit_directory.join(SERVICE_UNIT_NAME),
    )?;
    copy_unit(&sources.socket, &user_unit_directory.join(SOCKET_UNIT_NAME))?;
    Ok(())
}

fn copy_unit(source: &Path, destination: &Path) -> io::Result<()> {
    fs::copy(source, destination)?;
    fs::set_permissions(
        destination,
        fs::Permissions::from_mode(EXPECTED_UNIT_FILE_MODE),
    )
}

fn assert_installed_unit(source: &Path, destination: &Path) {
    assert!(destination.is_file());
    assert_eq!(
        fs::read(source).expect("source unit should be readable"),
        fs::read(destination).expect("installed unit should be readable")
    );
    let mode = fs::metadata(destination)
        .expect("installed unit metadata should be readable")
        .permissions()
        .mode()
        & UNIT_FILE_MODE_MASK;
    assert_eq!(mode, EXPECTED_UNIT_FILE_MODE);
}

fn systemd_source_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../systemd")
}

fn unique_temp_path(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("rbus-systemd-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    path
}
