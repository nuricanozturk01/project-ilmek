#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

use ilmek_protocol::frame::{MAX_BODY_LEN, MAX_METADATA_LEN, PREFIX_LEN};
use ilmek_protocol::{Frame, FrameDecoder, FrameError};
use proptest::prelude::*;

fn frame(metadata: &[u8], body: &[u8]) -> Frame {
    Frame {
        metadata: metadata.to_vec(),
        body: body.to_vec(),
    }
}

fn drain(decoder: &mut FrameDecoder) -> Vec<Frame> {
    std::iter::from_fn(|| decoder.next_frame().unwrap()).collect()
}

#[test]
fn prefix_is_magic_then_big_endian_lengths() {
    let wire = frame(b"{}", &[1, 2, 3]).encode().unwrap();
    assert_eq!(&wire[..PREFIX_LEN], b"ILM1\0\0\0\x02\0\0\0\x03");
}

#[test]
fn two_frames_in_one_read_are_split() {
    let mut wire = frame(b"{\"a\":1}", b"xy").encode().unwrap();
    wire.extend(frame(b"{\"b\":2}", b"").encode().unwrap());

    let mut decoder = FrameDecoder::new();
    decoder.push(&wire);
    let frames = drain(&mut decoder);

    assert_eq!(
        frames,
        vec![frame(b"{\"a\":1}", b"xy"), frame(b"{\"b\":2}", b"")]
    );
    decoder.finish().unwrap();
}

#[test]
fn byte_at_a_time_delivery_yields_the_same_frame() {
    let original = frame(b"{\"k\":\"v\"}", &[7; 300]);
    let mut decoder = FrameDecoder::new();
    let mut seen = Vec::new();
    for byte in original.encode().unwrap() {
        decoder.push(&[byte]);
        seen.extend(drain(&mut decoder));
    }
    assert_eq!(seen, vec![original]);
}

#[test]
fn bad_magic_is_detected_before_the_prefix_completes() {
    let mut decoder = FrameDecoder::new();
    decoder.push(b"IL");
    assert_eq!(decoder.next_frame().unwrap(), None);
    decoder.push(b"X");
    assert_eq!(decoder.next_frame().unwrap_err(), FrameError::BadMagic);
}

#[test]
fn oversize_lengths_fail_without_waiting_for_payload() {
    let mut decoder = FrameDecoder::new();
    decoder.push(b"ILM1\0\x01\0\x01\0\0\0\0");
    assert_eq!(
        decoder.next_frame().unwrap_err(),
        FrameError::MetadataLength(MAX_METADATA_LEN + 1)
    );

    let mut decoder = FrameDecoder::new();
    decoder.push(b"ILM1\0\0\0\x01\0\x04\0\x01");
    assert_eq!(
        decoder.next_frame().unwrap_err(),
        FrameError::BodyLength(MAX_BODY_LEN + 1)
    );
}

#[test]
fn zero_length_metadata_is_rejected() {
    let mut decoder = FrameDecoder::new();
    decoder.push(b"ILM1\0\0\0\0\0\0\0\0");
    assert_eq!(
        decoder.next_frame().unwrap_err(),
        FrameError::MetadataLength(0)
    );
}

#[test]
fn limit_sized_frame_is_accepted_and_one_more_byte_is_not_encodable() {
    let largest = frame(&vec![b' '; MAX_METADATA_LEN], &vec![0; MAX_BODY_LEN]);
    let mut decoder = FrameDecoder::new();
    decoder.push(&largest.encode().unwrap());
    assert_eq!(decoder.next_frame().unwrap(), Some(largest));

    assert_eq!(
        frame(b"{}", &vec![0; MAX_BODY_LEN + 1])
            .encode()
            .unwrap_err(),
        FrameError::BodyLength(MAX_BODY_LEN + 1)
    );
}

#[test]
fn eof_inside_a_frame_is_truncation() {
    let wire = frame(b"{}", b"abc").encode().unwrap();
    let mut decoder = FrameDecoder::new();
    decoder.push(&wire[..wire.len() - 1]);
    assert_eq!(decoder.next_frame().unwrap(), None);
    assert_eq!(decoder.finish().unwrap_err(), FrameError::Truncated);
}

proptest! {
    #[test]
    fn any_chunking_of_a_frame_sequence_decodes_identically(
        parts in prop::collection::vec(
            (prop::collection::vec(any::<u8>(), 1..64), prop::collection::vec(any::<u8>(), 0..64)),
            1..6,
        ),
        cuts in prop::collection::vec(1usize..40, 1..20),
    ) {
        let frames: Vec<Frame> = parts.into_iter().map(|(m, b)| Frame { metadata: m, body: b }).collect();
        let wire: Vec<u8> = frames.iter().flat_map(|f| f.encode().unwrap()).collect();

        let mut decoder = FrameDecoder::new();
        let mut decoded = Vec::new();
        let mut rest = wire.as_slice();
        for cut in cuts.iter().cycle() {
            if rest.is_empty() { break; }
            let (head, tail) = rest.split_at((*cut).min(rest.len()));
            decoder.push(head);
            decoded.extend(drain(&mut decoder));
            rest = tail;
        }
        prop_assert_eq!(decoded, frames);
        prop_assert!(decoder.finish().is_ok());
    }
}
