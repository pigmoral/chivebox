use std::io;
use std::mem::size_of;
use std::net::Ipv4Addr;
use std::os::fd::RawFd;

pub const NETLINK_ROUTE: libc::c_int = 0;

pub const RTM_NEWLINK: u16 = 16;
pub const RTM_DELLINK: u16 = 17;
pub const RTM_GETLINK: u16 = 18;
pub const RTM_NEWADDR: u16 = 20;
pub const RTM_DELADDR: u16 = 21;
pub const RTM_GETADDR: u16 = 22;
pub const RTM_NEWROUTE: u16 = 24;
pub const RTM_DELROUTE: u16 = 25;
pub const RTM_GETROUTE: u16 = 26;

pub const NLM_F_REQUEST: u16 = 0x01;
pub const NLM_F_MULTI: u16 = 0x02;
pub const NLM_F_ACK: u16 = 0x04;
pub const NLM_F_ECHO: u16 = 0x08;
pub const NLM_F_ROOT: u16 = 0x100;
pub const NLM_F_MATCH: u16 = 0x200;
pub const NLM_F_DUMP: u16 = NLM_F_ROOT | NLM_F_MATCH;
pub const NLM_F_REPLACE: u16 = 0x100;
pub const NLM_F_EXCL: u16 = 0x200;
pub const NLM_F_CREATE: u16 = 0x400;
pub const NLM_F_APPEND: u16 = 0x800;

pub const IFLA_ADDRESS: u16 = 1;
pub const IFLA_BROADCAST: u16 = 2;
pub const IFLA_IFNAME: u16 = 3;
pub const IFLA_MTU: u16 = 4;

pub const IFA_ADDRESS: u16 = 1;
pub const IFA_LOCAL: u16 = 2;
pub const IFA_LABEL: u16 = 3;
pub const IFA_BROADCAST: u16 = 4;

pub const RTA_DST: u16 = 1;
pub const RTA_SRC: u16 = 2;
pub const RTA_IIF: u16 = 3;
pub const RTA_OIF: u16 = 4;
pub const RTA_GATEWAY: u16 = 5;
pub const RTA_PRIORITY: u16 = 6;
pub const RTA_PREFSRC: u16 = 7;

pub const AF_INET_U8: u8 = libc::AF_INET as u8;
pub const AF_UNSPEC_U8: u8 = libc::AF_UNSPEC as u8;

pub const IFF_UP: u32 = 1;

pub const RTPROT_BOOT: u8 = 3;
pub const RTPROT_STATIC: u8 = 4;
pub const RT_SCOPE_UNIVERSE: u8 = 0;
pub const RT_SCOPE_NOWHERE: u8 = 255;
pub const RT_SCOPE_LINK: u8 = 253;
pub const RT_SCOPE_HOST: u8 = 254;
pub const RT_TABLE_MAIN: u8 = 254;
pub const RTN_UNICAST: u8 = 1;
pub const RTN_LOCAL: u8 = 2;
pub const RTN_BROADCAST: u8 = 3;
pub const RTN_ANYCAST: u8 = 4;
pub const RTN_MULTICAST: u8 = 5;
pub const RTN_BLACKHOLE: u8 = 6;
pub const RTN_UNREACHABLE: u8 = 7;
pub const RTN_PROHIBIT: u8 = 8;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct NlMsgHdr {
    pub nlmsg_len: u32,
    pub nlmsg_type: u16,
    pub nlmsg_flags: u16,
    pub nlmsg_seq: u32,
    pub nlmsg_pid: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct IfInfoMsg {
    pub ifi_family: u8,
    pub ifi_pad: u8,
    pub ifi_type: u16,
    pub ifi_index: i32,
    pub ifi_flags: u32,
    pub ifi_change: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct IfAddrMsg {
    pub ifa_family: u8,
    pub ifa_prefixlen: u8,
    pub ifa_flags: u8,
    pub ifa_scope: u8,
    pub ifa_index: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RtMsg {
    pub rtm_family: u8,
    pub rtm_dst_len: u8,
    pub rtm_src_len: u8,
    pub rtm_tos: u8,
    pub rtm_table: u8,
    pub rtm_protocol: u8,
    pub rtm_scope: u8,
    pub rtm_type: u8,
    pub rtm_flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RtAttr {
    pub rta_len: u16,
    pub rta_type: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RtMsgDefault {
    pub rtm_family: u8,
    pub rtm_dst_len: u8,
    pub rtm_src_len: u8,
    pub rtm_tos: u8,
    pub rtm_table: u8,
    pub rtm_protocol: u8,
    pub rtm_scope: u8,
    pub rtm_type: u8,
    pub rtm_flags: u32,
}

pub struct NetlinkSocket {
    fd: RawFd,
    seq: u32,
}

impl NetlinkSocket {
    pub fn connect() -> io::Result<Self> {
        let fd = unsafe { libc::socket(libc::AF_NETLINK, libc::SOCK_RAW, NETLINK_ROUTE) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        let mut addr: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
        addr.nl_family = libc::AF_NETLINK as u16;
        addr.nl_pid = 0;
        addr.nl_groups = 0;

        let ret = unsafe {
            libc::bind(
                fd,
                &addr as *const libc::sockaddr_nl as *const libc::sockaddr,
                size_of::<libc::sockaddr_nl>() as libc::socklen_t,
            )
        };

        if ret < 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(err);
        }

        Ok(Self { fd, seq: 1 })
    }

    pub fn next_seq(&mut self) -> u32 {
        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        seq
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }
}

impl Drop for NetlinkSocket {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd) };
    }
}

pub fn nlmsg_align(len: usize) -> usize {
    (len + 3) & !3
}

pub fn rta_align(len: usize) -> usize {
    (len + 3) & !3
}

pub fn put_struct<T: Copy>(buf: &mut Vec<u8>, value: &T) {
    let ptr = value as *const T as *const u8;
    let slice = unsafe { std::slice::from_raw_parts(ptr, size_of::<T>()) };
    buf.extend_from_slice(slice);
}

pub fn put_u32(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_ne_bytes());
}

pub fn put_u16(buf: &mut Vec<u8>, value: u16) {
    buf.extend_from_slice(&value.to_ne_bytes());
}

pub fn put_u8(buf: &mut Vec<u8>, value: u8) {
    buf.push(value);
}

pub fn put_bytes(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(bytes);
}

pub fn put_attr(buf: &mut Vec<u8>, attr_type: u16, payload: &[u8]) {
    let len = size_of::<RtAttr>() + payload.len();
    let hdr = RtAttr {
        rta_len: len as u16,
        rta_type: attr_type,
    };
    put_struct(buf, &hdr);
    buf.extend_from_slice(payload);
    let pad = rta_align(len) - len;
    if pad > 0 {
        buf.extend(std::iter::repeat_n(0u8, pad));
    }
}

pub fn send_netlink(
    fd: RawFd,
    payload: &[u8],
    msg_type: u16,
    flags: u16,
    seq: u32,
) -> io::Result<()> {
    let hdr = NlMsgHdr {
        nlmsg_len: (size_of::<NlMsgHdr>() + payload.len()) as u32,
        nlmsg_type: msg_type,
        nlmsg_flags: flags,
        nlmsg_seq: seq,
        nlmsg_pid: 0,
    };

    let mut buf = Vec::with_capacity(size_of::<NlMsgHdr>() + payload.len());
    put_struct(&mut buf, &hdr);
    buf.extend_from_slice(payload);

    let ret = unsafe { libc::send(fd, buf.as_ptr() as *const libc::c_void, buf.len(), 0) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn recv_messages(fd: RawFd) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; 32768];
    let ret = unsafe { libc::recv(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len(), 0) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        buf.truncate(ret as usize);
        Ok(buf)
    }
}

pub fn read_cstring(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).to_string()
}

pub fn attr_payload<'a>(attr: &'a RtAttr) -> &'a [u8] {
    let len = usize::from(attr.rta_len).saturating_sub(size_of::<RtAttr>());
    let ptr = attr as *const RtAttr as *const u8;
    unsafe { std::slice::from_raw_parts(ptr.add(size_of::<RtAttr>()), len) }
}

pub fn parse_attrs(mut bytes: &[u8]) -> Vec<(u16, Vec<u8>)> {
    let mut attrs = Vec::new();
    while bytes.len() >= size_of::<RtAttr>() {
        let hdr = unsafe { &*(bytes.as_ptr() as *const RtAttr) };
        let len = usize::from(hdr.rta_len);
        if len < size_of::<RtAttr>() || len > bytes.len() {
            break;
        }
        let payload = bytes[size_of::<RtAttr>()..len].to_vec();
        attrs.push((hdr.rta_type, payload));
        let aligned = rta_align(len);
        if aligned > bytes.len() {
            break;
        }
        bytes = &bytes[aligned..];
    }
    attrs
}

pub fn parse_nlmsgs(mut bytes: &[u8]) -> Vec<(NlMsgHdr, Vec<u8>)> {
    let mut msgs = Vec::new();
    while bytes.len() >= size_of::<NlMsgHdr>() {
        let hdr = unsafe { &*(bytes.as_ptr() as *const NlMsgHdr) };
        let len = usize::try_from(hdr.nlmsg_len).unwrap_or(0);
        if len < size_of::<NlMsgHdr>() || len > bytes.len() {
            break;
        }
        msgs.push((*hdr, bytes[size_of::<NlMsgHdr>()..len].to_vec()));
        let aligned = nlmsg_align(len);
        if aligned > bytes.len() {
            break;
        }
        bytes = &bytes[aligned..];
    }
    msgs
}

pub fn read_u32_ne(bytes: &[u8]) -> Option<u32> {
    if bytes.len() < 4 {
        None
    } else {
        Some(u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
}

pub fn read_i32_ne(bytes: &[u8]) -> Option<i32> {
    if bytes.len() < 4 {
        None
    } else {
        Some(i32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
}

pub fn read_u16_ne(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 2 {
        None
    } else {
        Some(u16::from_ne_bytes([bytes[0], bytes[1]]))
    }
}

pub fn parse_ipv4(bytes: &[u8]) -> Option<Ipv4Addr> {
    ipv4_from_be_bytes(bytes)
}

pub fn ipv4_from_be_bytes(bytes: &[u8]) -> Option<Ipv4Addr> {
    if bytes.len() < 4 {
        return None;
    }
    Some(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]))
}
