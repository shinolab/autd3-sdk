use autd3_cpu_wire::layout::UPDATE_CHUNK_MAX_DATA_LEN;
use autd3_cpu_wire::payload::{UpdateBeginPayload, UpdateChunkPayload};
use zerocopy::little_endian::U32;

use crate::datagram::Datagram;
use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::protocol::Cmd;

use super::{Operation, encode_header_only, set_cmd, write_header};

fn encode_begin(out: &mut Datagram, cmd: Cmd, length: u32, crc32: u32) {
    encode_header_only(
        out,
        cmd,
        &UpdateBeginPayload {
            length: U32::new(length),
            crc32: U32::new(crc32),
        },
    );
}

fn encode_chunk(out: &mut Datagram, cmd: Cmd, offset: u32, data: &[u8]) -> Result<(), Error> {
    if data.len() > UPDATE_CHUNK_MAX_DATA_LEN {
        return Err(PayloadError::UpdateChunkTooLong {
            len: data.len(),
            max: UPDATE_CHUNK_MAX_DATA_LEN,
        }
        .into());
    }
    let header = UpdateChunkPayload {
        offset: U32::new(offset),
    };
    write_header(out, &header)[..data.len()].copy_from_slice(data);
    set_cmd(out, cmd, size_of::<UpdateChunkPayload>() + data.len());
    Ok(())
}

macro_rules! begin_op {
    ($name:ident, $cmd:expr) => {
        #[doc(hidden)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $name {
            pub length: u32,
            pub crc32: u32,
        }

        impl crate::sealed::Sealed for $name {}

        impl Operation for $name {
            fn encode(&self, _device: &Device, out: &mut Datagram) -> Result<(), Error> {
                encode_begin(out, $cmd, self.length, self.crc32);
                Ok(())
            }
        }
    };
}

macro_rules! chunk_op {
    ($name:ident, $cmd:expr) => {
        #[doc(hidden)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $name<'a> {
            pub offset: u32,
            pub data: &'a [u8],
        }

        impl crate::sealed::Sealed for $name<'_> {}

        impl Operation for $name<'_> {
            fn encode(&self, _device: &Device, out: &mut Datagram) -> Result<(), Error> {
                encode_chunk(out, $cmd, self.offset, self.data)
            }
        }
    };
}

macro_rules! no_payload_op {
    ($name:ident, $cmd:expr) => {
        #[doc(hidden)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $name;

        impl crate::sealed::Sealed for $name {}

        impl Operation for $name {
            fn encode(&self, _device: &Device, out: &mut Datagram) -> Result<(), Error> {
                set_cmd(out, $cmd, 0);
                Ok(())
            }
        }
    };
}

begin_op!(UpdateBegin, Cmd::UpdateBegin);
begin_op!(FpgaUpdateBegin, Cmd::FpgaUpdateBegin);
chunk_op!(UpdateChunk, Cmd::UpdateChunk);
chunk_op!(FpgaUpdateChunk, Cmd::FpgaUpdateChunk);
no_payload_op!(UpdateCommit, Cmd::UpdateCommit);
no_payload_op!(UpdateConfirm, Cmd::UpdateConfirm);
no_payload_op!(Reboot, Cmd::Reboot);
no_payload_op!(FpgaUpdateCommit, Cmd::FpgaUpdateCommit);
no_payload_op!(FpgaUpdateActivate, Cmd::FpgaUpdateActivate);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::operation::Distribution;
    use crate::test_utils::test_device;

    #[test]
    fn begin_carries_the_length_and_the_crc() {
        for (op, cmd) in [
            (
                &UpdateBegin {
                    length: 0x0102_0304,
                    crc32: 0x0506_0708,
                } as &dyn Operation,
                Cmd::UpdateBegin,
            ),
            (
                &FpgaUpdateBegin {
                    length: 0x0102_0304,
                    crc32: 0x0506_0708,
                },
                Cmd::FpgaUpdateBegin,
            ),
        ] {
            let mut out = Datagram::no_payload(Cmd::Nop);
            op.encode(&test_device(0), &mut out).unwrap();
            assert_eq!(out.cmd, cmd);
            assert_eq!(out.payload(), [4, 3, 2, 1, 8, 7, 6, 5]);
        }
    }

    #[test]
    fn chunk_carries_the_offset_before_the_data() {
        for (op, cmd) in [
            (
                &UpdateChunk {
                    offset: 0x0102_0304,
                    data: &[9, 8],
                } as &dyn Operation,
                Cmd::UpdateChunk,
            ),
            (
                &FpgaUpdateChunk {
                    offset: 0x0102_0304,
                    data: &[9, 8],
                },
                Cmd::FpgaUpdateChunk,
            ),
        ] {
            let mut out = Datagram::no_payload(Cmd::Nop);
            op.encode(&test_device(0), &mut out).unwrap();
            assert_eq!(out.cmd, cmd);
            assert_eq!(out.payload(), [4, 3, 2, 1, 9, 8]);
        }
    }

    #[test]
    fn a_chunk_longer_than_the_wire_limit_is_rejected() {
        let max = UPDATE_CHUNK_MAX_DATA_LEN;
        let data = vec![0; max + 1];
        for op in [
            &UpdateChunk {
                offset: 0,
                data: &data,
            } as &dyn Operation,
            &FpgaUpdateChunk {
                offset: 0,
                data: &data,
            },
        ] {
            let mut out = Datagram::no_payload(Cmd::Nop);
            let err = op.encode(&test_device(0), &mut out).unwrap_err();
            assert!(matches!(
                err,
                Error::InvalidPayload(PayloadError::UpdateChunkTooLong { len, max: limit })
                    if len == max + 1 && limit == max
            ));
        }
        let mut out = Datagram::no_payload(Cmd::Nop);
        UpdateChunk {
            offset: 0,
            data: &data[..max],
        }
        .encode(&test_device(0), &mut out)
        .unwrap();
        assert_eq!(out.payload_len, 4 + max);
    }

    #[test]
    fn no_payload_ops_are_broadcast() {
        let ops: [(&dyn Operation, Cmd); 5] = [
            (&UpdateCommit, Cmd::UpdateCommit),
            (&UpdateConfirm, Cmd::UpdateConfirm),
            (&Reboot, Cmd::Reboot),
            (&FpgaUpdateCommit, Cmd::FpgaUpdateCommit),
            (&FpgaUpdateActivate, Cmd::FpgaUpdateActivate),
        ];
        for (op, cmd) in ops {
            let mut out = Datagram::no_payload(Cmd::Nop);
            op.encode(&test_device(0), &mut out).unwrap();
            assert_eq!((out.cmd, out.payload_len), (cmd, 0));
            assert_eq!(op.distribution(), Distribution::Broadcast);
        }
    }
}
