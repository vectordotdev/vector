use std::error::Error as StdError;
use std::ffi::CStr;
use std::fmt;
use std::ptr;
use crate::sources::lldp::bindings::*;

/// LLDP consts
pub const LLDPCTL_K_PORT_CHASSIS: lldpctl_key_t = 1208;

/// LLDP error
#[derive(Debug)]
pub enum LldpError {
    ConnectionFailed,
    InterfaceFetchFailed,
    NullPointer(&'static str),
    ThreadJoinFailed,
}

impl fmt::Display for LldpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LldpError::ConnectionFailed => {
                write!(f, "LLDP connection failed")
            }
            LldpError::InterfaceFetchFailed => write!(f, "Failed to fetch network interfaces"),
            LldpError::NullPointer(ctx) => write!(f, "Null pointer encountered in: {}", ctx),
            LldpError::ThreadJoinFailed => write!(f, "Blocking thread error"),
        }
    }
}

impl StdError for LldpError {}

/// LLDP data
#[derive(Debug, Clone)]
pub struct LldpInterface {
    pub name: String,
    pub device_name: String,
}

#[derive(Debug, Clone)]
pub struct LldpNeighbor {
    pub local_interface: String,
    pub local_device: String,
    pub remote_device: String,
    pub remote_port: String,
}

/// RAII
struct AtomGuard {
    ptr: *mut lldpctl_atom_t,
}

impl AtomGuard {
    fn new(ptr: *mut lldpctl_atom_t) -> Self {
        Self { ptr }
    }

    fn ptr(&self) -> *mut lldpctl_atom_t {
        self.ptr
    }
}

impl Drop for AtomGuard {
    fn drop(&mut self) {
        unsafe {
            lldpctl_atom_dec_ref(self.ptr);
        }
    }
}

pub struct LldpHandle {
    conn: *mut lldpctl_conn_t,
}

impl LldpHandle {
    pub fn new() -> Result<Self, LldpError> {
        unsafe {
            let conn = lldpctl_new(None, None, ptr::null_mut());
            if conn.is_null() {
                Err(LldpError::ConnectionFailed)
            } else {
                Ok(Self { conn })
            }
        }
    }

    pub fn get_interfaces(&self) -> Result<Vec<LldpInterface>, LldpError> {
        unsafe {
            let interfaces = lldpctl_get_interfaces(self.conn);
            if interfaces.is_null() {
                return Err(LldpError::InterfaceFetchFailed);
            }

            let interfaces = AtomGuard::new(interfaces);
            let mut result = Vec::new();

            let mut iter = lldpctl_atom_iter(interfaces.ptr());
            while !iter.is_null() {
                let interface_atom = lldpctl_atom_iter_value(interfaces.ptr(), iter);
                iter = lldpctl_atom_iter_next(interfaces.ptr(), iter);
                if interface_atom.is_null() {
                    continue;
                }

                let interface = AtomGuard::new(interface_atom);

                let name = get_string_property(interface.ptr(), lldpctl_key_t_lldpctl_k_interface_name)
                    .unwrap_or_default();

                let port_ptr = lldpctl_get_port(interface.ptr());
                if port_ptr.is_null() {
                    continue;
                }
                let port = AtomGuard::new(port_ptr);

                let chassis_ptr = lldpctl_atom_get(port.ptr(), LLDPCTL_K_PORT_CHASSIS);
                if chassis_ptr.is_null() {
                    continue;
                }

                let chassis = AtomGuard::new(chassis_ptr);
                let device_name =
                    get_string_property(chassis.ptr(), lldpctl_key_t_lldpctl_k_chassis_name).unwrap_or_default();

                result.push(LldpInterface {
                    name,
                    device_name,
                });
            }

            Ok(result)
        }
    }

    pub fn get_neighbors(&self) -> Result<Vec<LldpNeighbor>, LldpError> {
        unsafe {
            let interface_list = lldpctl_get_interfaces(self.conn);
            if interface_list.is_null() {
                return Err(LldpError::InterfaceFetchFailed);
            }
            let interface_list = AtomGuard::new(interface_list);
            let mut result = Vec::new();

            let mut iter = lldpctl_atom_iter(interface_list.ptr());
            while !iter.is_null() {
                let interface_ptr = lldpctl_atom_iter_value(interface_list.ptr(), iter);
                iter = lldpctl_atom_iter_next(interface_list.ptr(), iter);

                if interface_ptr.is_null() {
                    continue;
                }
                let interface = AtomGuard::new(interface_ptr);

                let interface_name = get_string_property(interface.ptr(), lldpctl_key_t_lldpctl_k_interface_name)
                    .unwrap_or_default();

                let port_ptr = lldpctl_get_port(interface.ptr());
                if port_ptr.is_null() {
                    continue;
                }
                let port = AtomGuard::new(port_ptr);

                let chassis_ptr = lldpctl_atom_get(port.ptr(), LLDPCTL_K_PORT_CHASSIS);
                if chassis_ptr.is_null() {
                    continue;
                }

                let chassis = AtomGuard::new(chassis_ptr);
                let local_chassis_name =
                    get_string_property(chassis.ptr(), lldpctl_key_t_lldpctl_k_chassis_name).unwrap_or_default();

                // 获取 neighbor 列表
                let neighbors_ptr = lldpctl_atom_get(port.ptr(), lldpctl_key_t_lldpctl_k_port_neighbors as lldpctl_key_t);
                if neighbors_ptr.is_null() {
                    continue;
                }
                let neighbors = AtomGuard::new(neighbors_ptr);

                // 遍历每个 neighbor
                let mut n_iter = lldpctl_atom_iter(neighbors.ptr());
                while !n_iter.is_null() {
                    let neighbor_ptr = lldpctl_atom_iter_value(neighbors.ptr(), n_iter);
                    n_iter = lldpctl_atom_iter_next(neighbors.ptr(), n_iter);

                    if neighbor_ptr.is_null() {
                        continue;
                    }

                    let neighbor = AtomGuard::new(neighbor_ptr);

                    let remote_device = get_string_property(neighbor.ptr(), lldpctl_key_t_lldpctl_k_chassis_name)
                        .unwrap_or_default();
                    let remote_port =
                        get_string_property(neighbor.ptr(), lldpctl_key_t_lldpctl_k_port_id).unwrap_or_default();

                    result.push(LldpNeighbor {
                        local_interface: interface_name.clone(),
                        local_device: local_chassis_name.clone(),
                        remote_device,
                        remote_port,
                    });
                }
            }

            Ok(result)
        }
    }
}

impl Drop for LldpHandle {
    fn drop(&mut self) {
        unsafe {
            lldpctl_release(self.conn);
        }
    }
}

fn get_string_property(atom: *mut lldpctl_atom_t, key: lldpctl_key_t) -> Result<String, LldpError> {
    unsafe {
        let cstr = lldpctl_atom_get_str(atom, key);

        if cstr.is_null() {
            Err(LldpError::NullPointer("get_string_property"))
        } else {
            let value = CStr::from_ptr(cstr).to_string_lossy().into_owned();
            Ok(value)
        }
    }
}
pub async fn get_lldp_interfaces_async() -> Result<Vec<LldpInterface>, LldpError> {
    tokio::task::spawn_blocking(|| {
        let handle = LldpHandle::new()?;
        handle.get_interfaces()
    })
    .await
    .map_err(|_| LldpError::ThreadJoinFailed)?
}

pub async fn get_lldp_neighbors_async() -> Result<Vec<LldpNeighbor>, LldpError> {
    tokio::task::spawn_blocking(|| {
        let handle = LldpHandle::new()?;
        handle.get_neighbors()
    })
    .await
    .map_err(|_| LldpError::ThreadJoinFailed)?
}
