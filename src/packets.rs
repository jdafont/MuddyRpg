use bytes::{BytesMut, BufMut, Bytes, Buf};
use tokio::{net::tcp::OwnedWriteHalf, io::AsyncWriteExt};

use crate::serialize::read_string;

#[non_exhaustive]
pub struct PacketType;

impl PacketType {
	pub const InitUserRequest: u32 = 0;
	pub const InitUserResponse: u32 = 1;
	pub const MoveRequest: u32 = 2;
	pub const MoveResponse: u32 = 3;
}

#[derive(Debug)]
pub enum Packet {
    InitUserRequest(InitUserRequestData),
    InitUserResponse(InitUserResponseData),
    MoveRequest(MoveRequestData),
    MoveResponse(MoveResponseData)
}

#[derive(Debug)]
pub struct InitUserRequestData {
    pub username: String
}

#[derive(Debug)]
pub struct InitUserResponseData {
    pub object_id: u32
}

#[derive(Debug)]
pub struct MoveRequestData {
    pub x: f32,
    pub y: f32
}

#[derive(Debug)]
pub struct MoveResponseData {
    pub confirm: bool,
    pub new_x: f32,
    pub new_y: f32
}

impl Packet {
    pub async fn send_packet(self, tx: &mut OwnedWriteHalf) -> std::result::Result<(), std::io::Error>{
        let mut buf = BytesMut::new();
        match self {
            Packet::InitUserResponse(data) => {
                buf.put_u32_le(1);
                buf.put_u32_le(data.object_id);
            },
            Packet::MoveResponse(data) => {
                buf.put_u8(if data.confirm { 1 } else { 0 });
                buf.put_f32_le(data.new_x);
                buf.put_f32_le(data.new_y);
            }
            _ => {}
        }

        return tx.write_all_buf(&mut buf).await;
    }

    pub fn read_packet(mut buf: Bytes) -> Option<Packet> {
        let packet_type = buf.get_u32_le();
        match packet_type {
            PacketType::InitUserRequest => {
                Some(Packet::InitUserRequest(
                    InitUserRequestData {
                        username: read_string(&mut buf) 
                    }
                ))
            },
            PacketType::MoveRequest => {
                Some(Packet::MoveRequest(MoveRequestData {
                    x: buf.get_f32_le(),
                    y: buf.get_f32_le()
                }))
            }
            _ => {
                return None
            }
        }
    }
}