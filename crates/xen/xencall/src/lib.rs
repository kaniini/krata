pub mod error;
pub mod sys;

use crate::error::{Error, Result};
use crate::sys::{
    AddressSize, CreateDomain, DomCtl, DomCtlValue, DomCtlVcpuContext, EvtChnAllocUnbound,
    GetDomainInfo, GetPageFrameInfo3, Hypercall, HypercallInit, MaxMem, MaxVcpus, MemoryMap,
    MemoryReservation, MmapBatch, MmapResource, MmuExtOp, MultiCallEntry, VcpuGuestContext,
    VcpuGuestContextAny, XenCapabilitiesInfo, HYPERVISOR_DOMCTL, HYPERVISOR_EVENT_CHANNEL_OP,
    HYPERVISOR_MEMORY_OP, HYPERVISOR_MMUEXT_OP, HYPERVISOR_MULTICALL, HYPERVISOR_XEN_VERSION,
    XENVER_CAPABILITIES, XEN_DOMCTL_CREATEDOMAIN, XEN_DOMCTL_DESTROYDOMAIN,
    XEN_DOMCTL_GETDOMAININFO, XEN_DOMCTL_GETPAGEFRAMEINFO3, XEN_DOMCTL_GETVCPUCONTEXT,
    XEN_DOMCTL_HYPERCALL_INIT, XEN_DOMCTL_MAX_MEM, XEN_DOMCTL_MAX_VCPUS, XEN_DOMCTL_PAUSEDOMAIN,
    XEN_DOMCTL_SETVCPUCONTEXT, XEN_DOMCTL_SET_ADDRESS_SIZE, XEN_DOMCTL_UNPAUSEDOMAIN,
    XEN_MEM_CLAIM_PAGES, XEN_MEM_MEMORY_MAP, XEN_MEM_POPULATE_PHYSMAP,
};
use libc::{c_int, mmap, usleep, MAP_FAILED, MAP_SHARED, PROT_READ, PROT_WRITE};
use log::trace;
use nix::errno::Errno;
use std::ffi::{c_long, c_uint, c_ulong, c_void};
use std::sync::Arc;
use sys::{XEN_DOMCTL_MAX_INTERFACE_VERSION, XEN_DOMCTL_MIN_INTERFACE_VERSION};
use tokio::sync::Semaphore;

use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::ptr::addr_of_mut;
use std::slice;

#[derive(Clone)]
pub struct XenCall {
    pub handle: Arc<File>,
    semaphore: Arc<Semaphore>,
    domctl_interface_version: u32,
}

impl XenCall {
    pub fn open(current_domid: u32) -> Result<XenCall> {
        let handle = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/xen/privcmd")?;
        let domctl_interface_version =
            XenCall::detect_domctl_interface_version(&handle, current_domid)?;
        Ok(XenCall {
            handle: Arc::new(handle),
            semaphore: Arc::new(Semaphore::new(1)),
            domctl_interface_version,
        })
    }

    fn detect_domctl_interface_version(handle: &File, current_domid: u32) -> Result<u32> {
        for version in XEN_DOMCTL_MIN_INTERFACE_VERSION..XEN_DOMCTL_MAX_INTERFACE_VERSION + 1 {
            let mut domctl = DomCtl {
                cmd: XEN_DOMCTL_GETDOMAININFO,
                interface_version: version,
                domid: current_domid,
                value: DomCtlValue {
                    get_domain_info: GetDomainInfo::default(),
                },
            };
            unsafe {
                let mut call = Hypercall {
                    op: HYPERVISOR_DOMCTL,
                    arg: [addr_of_mut!(domctl) as u64, 0, 0, 0, 0],
                };
                let result = sys::hypercall(handle.as_raw_fd(), &mut call).unwrap_or(-1);
                if result == 0 {
                    return Ok(version);
                }
            }
        }
        Err(Error::XenVersionUnsupported)
    }

    pub async fn mmap(&self, addr: u64, len: u64) -> Option<u64> {
        let _permit = self.semaphore.acquire().await.ok()?;
        trace!(
            "call fd={} mmap addr={:#x} len={}",
            self.handle.as_raw_fd(),
            addr,
            len
        );
        unsafe {
            let ptr = mmap(
                addr as *mut c_void,
                len as usize,
                PROT_READ | PROT_WRITE,
                MAP_SHARED,
                self.handle.as_raw_fd(),
                0,
            );
            if ptr == MAP_FAILED {
                None
            } else {
                trace!(
                    "call fd={} mmap addr={:#x} len={} = {:#x}",
                    self.handle.as_raw_fd(),
                    addr,
                    len,
                    ptr as u64,
                );
                Some(ptr as u64)
            }
        }
    }

    pub async fn hypercall(&self, op: c_ulong, arg: [c_ulong; 5]) -> Result<c_long> {
        let _permit = self.semaphore.acquire().await?;
        trace!(
            "call fd={} hypercall op={:#x} arg={:?}",
            self.handle.as_raw_fd(),
            op,
            arg
        );
        unsafe {
            let mut call = Hypercall { op, arg };
            let result = sys::hypercall(self.handle.as_raw_fd(), &mut call)?;
            Ok(result as c_long)
        }
    }

    pub async fn hypercall0(&self, op: c_ulong) -> Result<c_long> {
        self.hypercall(op, [0, 0, 0, 0, 0]).await
    }

    pub async fn hypercall1(&self, op: c_ulong, arg1: c_ulong) -> Result<c_long> {
        self.hypercall(op, [arg1, 0, 0, 0, 0]).await
    }

    pub async fn hypercall2(&self, op: c_ulong, arg1: c_ulong, arg2: c_ulong) -> Result<c_long> {
        self.hypercall(op, [arg1, arg2, 0, 0, 0]).await
    }

    pub async fn hypercall3(
        &self,
        op: c_ulong,
        arg1: c_ulong,
        arg2: c_ulong,
        arg3: c_ulong,
    ) -> Result<c_long> {
        self.hypercall(op, [arg1, arg2, arg3, 0, 0]).await
    }

    pub async fn hypercall4(
        &self,
        op: c_ulong,
        arg1: c_ulong,
        arg2: c_ulong,
        arg3: c_ulong,
        arg4: c_ulong,
    ) -> Result<c_long> {
        self.hypercall(op, [arg1, arg2, arg3, arg4, 0]).await
    }

    pub async fn hypercall5(
        &self,
        op: c_ulong,
        arg1: c_ulong,
        arg2: c_ulong,
        arg3: c_ulong,
        arg4: c_ulong,
        arg5: c_ulong,
    ) -> Result<c_long> {
        self.hypercall(op, [arg1, arg2, arg3, arg4, arg5]).await
    }

    pub async fn multicall(&self, calls: &mut [MultiCallEntry]) -> Result<()> {
        trace!(
            "call fd={} multicall calls={:?}",
            self.handle.as_raw_fd(),
            calls
        );
        self.hypercall2(
            HYPERVISOR_MULTICALL,
            calls.as_mut_ptr() as c_ulong,
            calls.len() as c_ulong,
        )
        .await?;
        Ok(())
    }

    pub async fn map_resource(
        &self,
        domid: u32,
        typ: u32,
        id: u32,
        idx: u32,
        num: u64,
        addr: u64,
    ) -> Result<()> {
        let _permit = self.semaphore.acquire().await?;
        let mut resource = MmapResource {
            dom: domid as u16,
            typ,
            id,
            idx,
            num,
            addr,
        };
        unsafe {
            sys::mmap_resource(self.handle.as_raw_fd(), &mut resource)?;
        }
        Ok(())
    }

    pub async fn mmap_batch(
        &self,
        domid: u32,
        num: u64,
        addr: u64,
        mfns: Vec<u64>,
    ) -> Result<c_long> {
        let _permit = self.semaphore.acquire().await?;
        trace!(
            "call fd={} mmap_batch domid={} num={} addr={:#x} mfns={:?}",
            self.handle.as_raw_fd(),
            domid,
            num,
            addr,
            mfns
        );
        unsafe {
            let mut mfns = mfns.clone();
            let mut errors = vec![0i32; mfns.len()];
            let mut batch = MmapBatch {
                num: num as u32,
                domid: domid as u16,
                addr,
                mfns: mfns.as_mut_ptr(),
                errors: errors.as_mut_ptr(),
            };

            let result = sys::mmapbatch(self.handle.as_raw_fd(), &mut batch);
            if let Err(errno) = result {
                if errno != Errno::ENOENT {
                    return Err(Error::MmapBatchFailed(errno))?;
                }

                usleep(100);

                let mut i: usize = 0;
                let mut paged: usize = 0;
                loop {
                    if errors[i] != libc::ENOENT {
                        i += 1;
                        continue;
                    }

                    paged += 1;
                    let mut batch = MmapBatch {
                        num: 1,
                        domid: domid as u16,
                        addr: addr + ((i as u64) << 12),
                        mfns: mfns.as_mut_ptr().add(i),
                        errors: errors.as_mut_ptr().add(i),
                    };

                    loop {
                        i += 1;
                        if i < num as usize {
                            if errors[i] != libc::ENOENT {
                                break;
                            }
                            batch.num += 1;
                        }
                    }

                    let result = sys::mmapbatch(self.handle.as_raw_fd(), &mut batch);
                    if let Err(n) = result {
                        if n != Errno::ENOENT {
                            return Err(Error::MmapBatchFailed(n))?;
                        }
                    }

                    if i < num as usize {
                        break;
                    }

                    let count = result.unwrap();
                    if count <= 0 {
                        break;
                    }
                }

                return Ok(paged as c_long);
            }
            Ok(result.unwrap() as c_long)
        }
    }

    pub async fn get_version_capabilities(&self) -> Result<XenCapabilitiesInfo> {
        trace!(
            "call fd={} get_version_capabilities",
            self.handle.as_raw_fd()
        );
        let mut info = XenCapabilitiesInfo {
            capabilities: [0; 1024],
        };
        self.hypercall2(
            HYPERVISOR_XEN_VERSION,
            XENVER_CAPABILITIES,
            addr_of_mut!(info) as c_ulong,
        )
        .await?;
        Ok(info)
    }

    pub async fn evtchn_op(&self, cmd: c_int, arg: u64) -> Result<()> {
        self.hypercall2(HYPERVISOR_EVENT_CHANNEL_OP, cmd as c_ulong, arg)
            .await?;
        Ok(())
    }

    pub async fn evtchn_alloc_unbound(&self, domid: u32, remote_domid: u32) -> Result<u32> {
        let mut alloc_unbound = EvtChnAllocUnbound {
            dom: domid as u16,
            remote_dom: remote_domid as u16,
            port: 0,
        };
        self.evtchn_op(6, addr_of_mut!(alloc_unbound) as c_ulong)
            .await?;
        Ok(alloc_unbound.port)
    }

    pub async fn get_domain_info(&self, domid: u32) -> Result<GetDomainInfo> {
        trace!(
            "domctl fd={} get_domain_info domid={}",
            self.handle.as_raw_fd(),
            domid
        );
        let mut domctl = DomCtl {
            cmd: XEN_DOMCTL_GETDOMAININFO,
            interface_version: self.domctl_interface_version,
            domid,
            value: DomCtlValue {
                get_domain_info: GetDomainInfo::default(),
            },
        };
        self.hypercall1(HYPERVISOR_DOMCTL, addr_of_mut!(domctl) as c_ulong)
            .await?;
        Ok(unsafe { domctl.value.get_domain_info })
    }

    pub async fn create_domain(&self, create_domain: CreateDomain) -> Result<u32> {
        trace!(
            "domctl fd={} create_domain create_domain={:?}",
            self.handle.as_raw_fd(),
            create_domain
        );
        let mut domctl = DomCtl {
            cmd: XEN_DOMCTL_CREATEDOMAIN,
            interface_version: self.domctl_interface_version,
            domid: 0,
            value: DomCtlValue { create_domain },
        };
        self.hypercall1(HYPERVISOR_DOMCTL, addr_of_mut!(domctl) as c_ulong)
            .await?;
        Ok(domctl.domid)
    }

    pub async fn pause_domain(&self, domid: u32) -> Result<()> {
        trace!(
            "domctl fd={} pause_domain domid={:?}",
            self.handle.as_raw_fd(),
            domid,
        );
        let mut domctl = DomCtl {
            cmd: XEN_DOMCTL_PAUSEDOMAIN,
            interface_version: self.domctl_interface_version,
            domid,
            value: DomCtlValue { pad: [0; 128] },
        };
        self.hypercall1(HYPERVISOR_DOMCTL, addr_of_mut!(domctl) as c_ulong)
            .await?;
        Ok(())
    }

    pub async fn unpause_domain(&self, domid: u32) -> Result<()> {
        trace!(
            "domctl fd={} unpause_domain domid={:?}",
            self.handle.as_raw_fd(),
            domid,
        );
        let mut domctl = DomCtl {
            cmd: XEN_DOMCTL_UNPAUSEDOMAIN,
            interface_version: self.domctl_interface_version,
            domid,
            value: DomCtlValue { pad: [0; 128] },
        };
        self.hypercall1(HYPERVISOR_DOMCTL, addr_of_mut!(domctl) as c_ulong)
            .await?;
        Ok(())
    }

    pub async fn set_max_mem(&self, domid: u32, memkb: u64) -> Result<()> {
        trace!(
            "domctl fd={} set_max_mem domid={} memkb={}",
            self.handle.as_raw_fd(),
            domid,
            memkb
        );
        let mut domctl = DomCtl {
            cmd: XEN_DOMCTL_MAX_MEM,
            interface_version: self.domctl_interface_version,
            domid,
            value: DomCtlValue {
                max_mem: MaxMem { max_memkb: memkb },
            },
        };
        self.hypercall1(HYPERVISOR_DOMCTL, addr_of_mut!(domctl) as c_ulong)
            .await?;
        Ok(())
    }

    pub async fn set_max_vcpus(&self, domid: u32, max_vcpus: u32) -> Result<()> {
        trace!(
            "domctl fd={} set_max_vcpus domid={} max_vcpus={}",
            self.handle.as_raw_fd(),
            domid,
            max_vcpus
        );
        let mut domctl = DomCtl {
            cmd: XEN_DOMCTL_MAX_VCPUS,
            interface_version: self.domctl_interface_version,
            domid,
            value: DomCtlValue {
                max_cpus: MaxVcpus { max_vcpus },
            },
        };
        self.hypercall1(HYPERVISOR_DOMCTL, addr_of_mut!(domctl) as c_ulong)
            .await?;
        Ok(())
    }

    pub async fn set_address_size(&self, domid: u32, size: u32) -> Result<()> {
        trace!(
            "domctl fd={} set_address_size domid={} size={}",
            self.handle.as_raw_fd(),
            domid,
            size,
        );
        let mut domctl = DomCtl {
            cmd: XEN_DOMCTL_SET_ADDRESS_SIZE,
            interface_version: self.domctl_interface_version,
            domid,
            value: DomCtlValue {
                address_size: AddressSize { size },
            },
        };
        self.hypercall1(HYPERVISOR_DOMCTL, addr_of_mut!(domctl) as c_ulong)
            .await?;
        Ok(())
    }

    pub async fn get_vcpu_context(&self, domid: u32, vcpu: u32) -> Result<VcpuGuestContext> {
        trace!(
            "domctl fd={} get_vcpu_context domid={}",
            self.handle.as_raw_fd(),
            domid,
        );
        let mut wrapper = VcpuGuestContextAny {
            value: VcpuGuestContext::default(),
        };
        let mut domctl = DomCtl {
            cmd: XEN_DOMCTL_GETVCPUCONTEXT,
            interface_version: self.domctl_interface_version,
            domid,
            value: DomCtlValue {
                vcpu_context: DomCtlVcpuContext {
                    vcpu,
                    ctx: addr_of_mut!(wrapper) as c_ulong,
                },
            },
        };
        self.hypercall1(HYPERVISOR_DOMCTL, addr_of_mut!(domctl) as c_ulong)
            .await?;
        Ok(unsafe { wrapper.value })
    }

    pub async fn set_vcpu_context(
        &self,
        domid: u32,
        vcpu: u32,
        context: &VcpuGuestContext,
    ) -> Result<()> {
        trace!(
            "domctl fd={} set_vcpu_context domid={} context={:?}",
            self.handle.as_raw_fd(),
            domid,
            context,
        );

        let mut value = VcpuGuestContextAny { value: *context };
        let mut domctl = DomCtl {
            cmd: XEN_DOMCTL_SETVCPUCONTEXT,
            interface_version: self.domctl_interface_version,
            domid,
            value: DomCtlValue {
                vcpu_context: DomCtlVcpuContext {
                    vcpu,
                    ctx: addr_of_mut!(value) as c_ulong,
                },
            },
        };
        self.hypercall1(HYPERVISOR_DOMCTL, addr_of_mut!(domctl) as c_ulong)
            .await?;
        Ok(())
    }

    pub async fn get_page_frame_info(&self, domid: u32, frames: &[u64]) -> Result<Vec<u64>> {
        let mut buffer: Vec<u64> = frames.to_vec();
        let mut domctl = DomCtl {
            cmd: XEN_DOMCTL_GETPAGEFRAMEINFO3,
            interface_version: self.domctl_interface_version,
            domid,
            value: DomCtlValue {
                get_page_frame_info: GetPageFrameInfo3 {
                    num: buffer.len() as u64,
                    array: buffer.as_mut_ptr() as c_ulong,
                },
            },
        };
        self.hypercall1(HYPERVISOR_DOMCTL, addr_of_mut!(domctl) as c_ulong)
            .await?;
        let slice = unsafe {
            slice::from_raw_parts_mut(
                domctl.value.get_page_frame_info.array as *mut u64,
                domctl.value.get_page_frame_info.num as usize,
            )
        };
        Ok(slice.to_vec())
    }

    pub async fn hypercall_init(&self, domid: u32, gmfn: u64) -> Result<()> {
        trace!(
            "domctl fd={} hypercall_init domid={} gmfn={}",
            self.handle.as_raw_fd(),
            domid,
            gmfn
        );
        let mut domctl = DomCtl {
            cmd: XEN_DOMCTL_HYPERCALL_INIT,
            interface_version: self.domctl_interface_version,
            domid,
            value: DomCtlValue {
                hypercall_init: HypercallInit { gmfn },
            },
        };
        self.hypercall1(HYPERVISOR_DOMCTL, addr_of_mut!(domctl) as c_ulong)
            .await?;
        Ok(())
    }

    pub async fn destroy_domain(&self, domid: u32) -> Result<()> {
        trace!(
            "domctl fd={} destroy_domain domid={}",
            self.handle.as_raw_fd(),
            domid
        );
        let mut domctl = DomCtl {
            cmd: XEN_DOMCTL_DESTROYDOMAIN,
            interface_version: self.domctl_interface_version,
            domid,
            value: DomCtlValue { pad: [0; 128] },
        };
        self.hypercall1(HYPERVISOR_DOMCTL, addr_of_mut!(domctl) as c_ulong)
            .await?;
        Ok(())
    }

    pub async fn get_memory_map(&self, size_of_entry: usize) -> Result<Vec<u8>> {
        let mut memory_map = MemoryMap {
            count: 0,
            buffer: 0,
        };
        self.hypercall2(
            HYPERVISOR_MEMORY_OP,
            XEN_MEM_MEMORY_MAP as c_ulong,
            addr_of_mut!(memory_map) as c_ulong,
        )
        .await?;
        let mut buffer = vec![0u8; memory_map.count as usize * size_of_entry];
        memory_map.buffer = buffer.as_mut_ptr() as c_ulong;
        self.hypercall2(
            HYPERVISOR_MEMORY_OP,
            XEN_MEM_MEMORY_MAP as c_ulong,
            addr_of_mut!(memory_map) as c_ulong,
        )
        .await?;
        Ok(buffer)
    }

    pub async fn populate_physmap(
        &self,
        domid: u32,
        nr_extents: u64,
        extent_order: u32,
        mem_flags: u32,
        extent_starts: &[u64],
    ) -> Result<Vec<u64>> {
        trace!("memory fd={} populate_physmap domid={} nr_extents={} extent_order={} mem_flags={} extent_starts={:?}", self.handle.as_raw_fd(), domid, nr_extents, extent_order, mem_flags, extent_starts);
        let mut extent_starts = extent_starts.to_vec();
        let ptr = extent_starts.as_mut_ptr();

        let mut reservation = MemoryReservation {
            extent_start: ptr as c_ulong,
            nr_extents,
            extent_order,
            mem_flags,
            domid: domid as u16,
        };

        let calls = &mut [MultiCallEntry {
            op: HYPERVISOR_MEMORY_OP,
            result: 0,
            args: [
                XEN_MEM_POPULATE_PHYSMAP as c_ulong,
                addr_of_mut!(reservation) as c_ulong,
                0,
                0,
                0,
                0,
            ],
        }];
        self.multicall(calls).await?;
        let code = calls[0].result;
        if code > !0xfff {
            return Err(Error::PopulatePhysmapFailed);
        }
        if code as usize > extent_starts.len() {
            return Err(Error::PopulatePhysmapFailed);
        }
        let extents = extent_starts[0..code as usize].to_vec();
        Ok(extents)
    }

    pub async fn claim_pages(&self, domid: u32, pages: u64) -> Result<()> {
        trace!(
            "memory fd={} claim_pages domid={} pages={}",
            self.handle.as_raw_fd(),
            domid,
            pages
        );
        let mut reservation = MemoryReservation {
            extent_start: 0,
            nr_extents: pages,
            extent_order: 0,
            mem_flags: 0,
            domid: domid as u16,
        };
        self.hypercall2(
            HYPERVISOR_MEMORY_OP,
            XEN_MEM_CLAIM_PAGES as c_ulong,
            addr_of_mut!(reservation) as c_ulong,
        )
        .await?;
        Ok(())
    }

    pub async fn mmuext(&self, domid: u32, cmd: c_uint, arg1: u64, arg2: u64) -> Result<()> {
        let mut ops = MmuExtOp { cmd, arg1, arg2 };

        self.hypercall4(
            HYPERVISOR_MMUEXT_OP,
            addr_of_mut!(ops) as c_ulong,
            1,
            0,
            domid as c_ulong,
        )
        .await
        .map(|_| ())
    }
}
