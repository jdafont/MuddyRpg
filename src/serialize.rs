use bytes::{Bytes, BytesMut, BufMut, Buf};

pub fn write_string(buf: &mut BytesMut, value: &str) {
    buf.put_u32_le(value.len() as u32);
    buf.put_slice(value.as_bytes());
}

pub fn read_string(buf: &mut Bytes) -> String {
    let len = buf.get_u32_le();
    let str = buf.copy_to_bytes(len as usize);

    return String::from_utf8_lossy(&str).to_string();
}