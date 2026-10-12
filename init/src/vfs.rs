/*

function calls are port messages. scalars are serialized, and handles are passed.
the first handle is a reply port, which will receive the result of the function call.
TODO: detect when the other side is dead and close the port.

VfsFile:
    open(path: &str, flags: u32) -> Result<Handle<VfsFile>, VfsError>
    read(offset: u64, len: usize, buf: Region) -> Result<usize, VfsError>
    write(offset: u64, len: usize, buf: Region) -> Result<usize, VfsError>
    list(offset: u64, buf: Region) -> Result<usize, VfsError>
    unlink(name: &str) -> Result<(), VfsError>
    stat() -> Result<VfsStat, VfsError>
    rename(new_dir: Handle<VfsFile>, new_name: &str) -> Result<(), VfsError>
    sync() -> Result<(), VfsError>

*/

use alloc::{format, string::String, vec, vec::Vec};
use core::fmt::Debug;
use kernel_api::{
    println,
    user::{control_thread, create_thread, port_create, port_recv, port_send, sleep, Handle},
    ControlThreadOp, HandleId, KError,
};
use serde::{de::DeserializeOwned, Serialize};

pub mod api {
    use alloc::string::String;
    use serde::{Deserialize, Serialize};

    type VfsError = u32;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct VfsStat {
        // ...
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub enum VfsFileReq {
        Open {
            path: String,
            flags: u32,
        },
        Read {
            offset: u64,
            len: usize,
            // buf: Region,
        },
        Write {
            offset: u64,
            len: usize,
            // buf: Region,
        },
        List {
            offset: u64,
            // buf: Region,
        },
        Unlink {
            name: String,
        },
        Stat,
        Rename {
            // new_dir: VfsFile,
            new_name: String,
        },
        Sync,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct VfsFileOpenResp {
        pub error: VfsError,
        // file: VfsFile,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct VfsFileReadResp {
        pub error: VfsError,
        pub bytes_read: usize,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct VfsFileWriteResp {
        pub error: VfsError,
        pub bytes_written: usize,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct VfsFileListResp {
        pub error: VfsError,
        pub entries: usize,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct VfsFileUnlinkResp {
        pub error: VfsError,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct VfsFileStatResp {
        pub error: VfsError,
        pub stat: VfsStat,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct VfsFileRenameResp {
        pub error: VfsError,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct VfsFileSyncResp {
        pub error: VfsError,
    }
}

struct ConnState {
    path: String,
}

fn vfs_loop(input: Handle) {
    let mut ipc_loop = IpcLoop::new(input, ConnState { path: "".into() });
    loop {
        ipc_loop
            .handle_next(|conn, req, handles, reply_port, new_conn| match req {
                api::VfsFileReq::Open { path, flags } => {
                    if path.is_empty() {
                        panic!("path is empty");
                    }
                    assert_eq!(handles.len(), 0);

                    let new_path = if conn.path.is_empty() {
                        path
                    } else {
                        format!("{}/{}", conn.path, path)
                    };
                    let (new_handle_recv, new_handle_send) =
                        port_create().expect("Failed to create new handle");
                    println!("opening file: {:?}", new_path);

                    // Send response
                    ipc_response(
                        &reply_port,
                        &api::VfsFileOpenResp { error: 0 },
                        &[new_handle_send],
                    )
                    .expect("Failed to send response");

                    *new_conn = Some((new_handle_recv, ConnState { path: new_path }));
                    Ok(())
                }
                other => {
                    println!("unhandled request: {:?}", other);
                    Err(())
                }
            })
            .unwrap();
    }
}

pub fn spawn_vfs_server() -> Result<Handle, KError> {
    let (vfs_recv, vfs_send) = port_create()?;
    let tid = create_thread(move || vfs_loop(vfs_recv))?;
    control_thread(tid, ControlThreadOp::Resume)?;
    Ok(vfs_send)
}

pub struct IpcLoop<State> {
    conn: Vec<(Handle, State)>,
    recv_buf: Vec<u8>,
}

impl<State> IpcLoop<State> {
    pub fn new(input: Handle, input_state: State) -> Self {
        Self {
            conn: vec![(input, input_state)],
            recv_buf: vec![0; 1024],
        }
    }

    pub fn handle_next<Req: DeserializeOwned + Debug>(
        &mut self,
        handler: impl Fn(
            &mut State,
            Req,
            Vec<Handle>,
            Handle,
            &mut Option<(Handle, State)>,
        ) -> Result<(), ()>,
    ) -> Result<(), KError> {
        loop {
            // TODO: use waiter
            let mut new_conn = None;
            for i in 0.. {
                if let Some(new_conn) = new_conn.take() {
                    self.conn.push(new_conn)
                }
                let Some(conn) = self.conn.get_mut(i) else {
                    break;
                };
                let mut handles = [0; 8];
                let (bytes_read, handles_read) =
                    match port_recv(&conn.0, 0, &mut self.recv_buf, &mut handles) {
                        Ok((bytes_read, handles_read)) => (bytes_read, handles_read),
                        Err(KError::PortEmpty) => {
                            continue;
                        }
                        Err(e) => {
                            println!("error on {:?}: {:?}", conn.0, e);
                            self.conn.swap_remove(i);
                            continue;
                        }
                    };

                if handles_read == 0 {
                    // No reply port, ignore this message
                    println!("no reply port from {:?}, ignoring message", conn.0);
                    continue;
                }

                // Deserialize request
                let req: Req = postcard::from_bytes(&self.recv_buf[..bytes_read])
                    .expect("Failed to deserialize request");
                println!("vfs request: {:?}", req);

                // Handle request
                if let Err(_) = handler(
                    &mut conn.1,
                    req,
                    handles[1..handles_read]
                        .into_iter()
                        .map(|h| Handle::from_u64(*h))
                        .collect(),
                    Handle::from_u64(handles[0]),
                    &mut new_conn,
                ) {
                    println!(
                        "error handling request from {:?} with reply port {:?}",
                        conn.0, handles[0]
                    );
                    self.conn.swap_remove(i);
                    continue;
                }
            }
            if let Some(new_conn) = new_conn.take() {
                self.conn.push(new_conn)
            }
        }
    }
}

pub fn ipc_response(port: &Handle, res: &impl Serialize, handles: &[Handle]) -> Result<(), KError> {
    let res_bytes = postcard::to_allocvec(&res).expect("Failed to serialize response");
    port_send(
        port,
        0,
        &res_bytes,
        &handles.iter().map(|h| h.as_u64()).collect::<Vec<_>>(),
    )
    .expect("Failed to send response");
    Ok(())
}

pub fn ipc_request<R: DeserializeOwned>(
    port: &Handle,
    req: &impl Serialize,
    handles: &[Handle],
    expected_reply_handles: usize,
) -> Result<(R, Vec<Handle>), KError> {
    // Serialize request
    let req_bytes = postcard::to_allocvec(&req).expect("Failed to serialize request");

    // Send request
    let (reply_recv, reply_send) = port_create().expect("Failed to create reply port");
    let send_handles: Vec<HandleId> = Some(reply_send.as_u64())
        .into_iter()
        .chain(handles.iter().map(|h| h.as_u64()))
        .collect();
    port_send(port, 0, &req_bytes, &send_handles).expect("Failed to send request");

    // Receive reply
    let mut reply_bytes = [0; 1024];
    let mut reply_handles = vec![0; expected_reply_handles];
    let reply_bytes_read = loop {
        match port_recv(&reply_recv, 0, &mut reply_bytes, &mut reply_handles) {
            Ok((reply_bytes_read, reply_handles_read)) => {
                if reply_handles_read != expected_reply_handles {
                    return Err(KError::TooSmall);
                }
                break reply_bytes_read;
            }
            Err(KError::PortEmpty) => {
                continue;
            }
            Err(e) => {
                return Err(e);
            }
        }
    };

    // Deserialize reply
    let reply = postcard::from_bytes(&reply_bytes[..reply_bytes_read])
        .expect("Failed to deserialize reply");
    Ok((
        reply,
        reply_handles.into_iter().map(Handle::from_u64).collect(),
    ))
}

pub fn vfs_test(vfs_port: Handle) {
    let req1 = api::VfsFileReq::Open {
        path: "data".into(),
        flags: 0,
    };
    let res1 = ipc_request::<api::VfsFileOpenResp>(&vfs_port, &req1, &[], 1)
        .expect("Failed to send request");
    println!("open response: {:?}", res1);

    let req2 = api::VfsFileReq::Open {
        path: "test.txt".into(),
        flags: 0,
    };
    let res2 = ipc_request::<api::VfsFileOpenResp>(&res1.1[0], &req2, &[], 1)
        .expect("Failed to send request");
    println!("open response: {:?}", res2);
}
