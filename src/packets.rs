use bytes::{BytesMut, BufMut, Bytes, Buf};
use tokio::{net::tcp::OwnedWriteHalf, io::AsyncWriteExt};

use crate::serialize::{read_string, write_string};

#[non_exhaustive]
pub struct PacketType;

impl PacketType {
	pub const InitUserRequest: u32 = 0;
	pub const InitUserResponse: u32 = 1;
	pub const MoveRequest: u32 = 2;
	pub const MoveResponse: u32 = 3;
    pub const NewObjectEvent: u32 = 4;
    pub const ObjectMovedEvent: u32 = 5;
    pub const ObjectRemovedEvent: u32 = 6;
}

#[derive(Debug)]
pub enum Packet {
    InitUserRequest(InitUserRequestData),
    InitUserResponse(InitUserResponseData),
    MoveRequest(MoveRequestData),
    MoveResponse(MoveResponseData),
    NewObjectEvent(NewObjectEventData),
    ObjectMovedEvent(ObjectMovedEventData),
    ObjectRemovedEvent(ObjectRemovedEventData)
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
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool
}

#[derive(Debug)]
pub struct MoveResponseData {
    pub confirm: bool,
    pub new_x: f32,
    pub new_y: f32
}

#[derive(Debug)]
pub struct NewObjectEventData {
    pub object_id: u32,
    pub x: f32,
    pub y: f32,
    pub name: String
}

#[derive(Debug)]
pub struct ObjectMovedEventData {
    pub object_id: u32,
    pub x: f32,
    pub y: f32
}

#[derive(Debug)]
pub struct ObjectRemovedEventData {
    pub object_id: u32
}

impl Packet {
    pub async fn send_packet(&self, tx: &mut OwnedWriteHalf) -> std::result::Result<(), std::io::Error>{
        let mut buf = BytesMut::new();
        match self {
            Packet::InitUserResponse(data) => {
                buf.put_u32_le(PacketType::InitUserResponse);
                buf.put_u32_le(data.object_id);
            },
            Packet::MoveResponse(data) => {
                buf.put_u32_le(PacketType::MoveResponse);
                buf.put_u8(if data.confirm { 1 } else { 0 });
                buf.put_f32_le(data.new_x);
                buf.put_f32_le(data.new_y);
            },
            Packet::NewObjectEvent(data) => {
                buf.put_u32_le(PacketType::NewObjectEvent);
                buf.put_u32_le(data.object_id);
                buf.put_f32_le(data.x);
                buf.put_f32_le(data.y);
                write_string(&mut buf, &data.name);
            },
            Packet::ObjectMovedEvent(data) => {
                buf.put_u32_le(PacketType::ObjectMovedEvent);
                buf.put_u32_le(data.object_id);
                buf.put_f32_le(data.x);
                buf.put_f32_le(data.y);
            },
            Packet::ObjectRemovedEvent(data) => {
                buf.put_u32_le(PacketType::ObjectRemovedEvent);
                buf.put_u32_le(data.object_id);
            }
            _ => {}
        }

        let r = tx.write_all_buf(&mut buf).await;
        let r2 = tx.flush().await;
        return r2;
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
                    up: buf.get_u8() == 1,
                    down: buf.get_u8() == 1,
                    left: buf.get_u8() == 1,
                    right: buf.get_u8() == 1
                }))
            }
            _ => {
                return None
            }
        }
    }
}