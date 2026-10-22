use std::time::Duration;

use ilmek_core::transport::{FrameReader, Priority, TransportError, spawn_writer};
use ilmek_protocol::frame::MAX_BODY_LEN;
use ilmek_protocol::{Frame, FrameError};
use tokio::io::{AsyncWriteExt, duplex};
use tokio::time::timeout;

fn frame(tag: u8, body_len: usize) -> Frame {
    Frame {
        metadata: format!("{{\"t\":{tag}}}").into_bytes(),
        body: vec![tag; body_len],
    }
}

#[tokio::test]
async fn reader_reassembles_frames_from_small_writes() {
    let (mut tx, rx) = duplex(1024);
    let mut reader = FrameReader::new(rx);
    let expected = frame(7, 500);
    let wire = expected.encode().unwrap();

    let writer = tokio::spawn(async move {
        for piece in wire.chunks(13) {
            tx.write_all(piece).await.unwrap();
        }
        tx.shutdown().await.unwrap();
    });

    assert_eq!(reader.next().await.unwrap(), Some(expected));
    assert!(reader.next().await.unwrap().is_none());
    writer.await.unwrap();
}

#[tokio::test]
async fn eof_inside_a_frame_is_an_error() {
    let (mut tx, rx) = duplex(1024);
    let wire = frame(1, 10).encode().unwrap();
    tx.write_all(&wire[..wire.len() - 1]).await.unwrap();
    drop(tx);

    let error = FrameReader::new(rx).next().await.unwrap_err();
    assert!(
        matches!(error, TransportError::Frame(FrameError::Truncated)),
        "{error:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_started_frame_must_finish_within_thirty_seconds_even_with_trickling_bytes() {
    let (mut tx, rx) = duplex(1024);
    let wire = frame(1, 100).encode().unwrap();
    let trickle = tokio::spawn(async move {
        for byte in wire {
            tx.write_all(&[byte]).await.unwrap();
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        tx
    });

    let error = FrameReader::new(rx).next().await.unwrap_err();
    assert!(matches!(error, TransportError::FrameTimeout), "{error:?}");
    trickle.abort();
}

#[tokio::test]
async fn idle_connection_without_a_started_frame_does_not_time_out_in_the_reader() {
    let (_tx, rx) = duplex(1024);
    let mut reader = FrameReader::new(rx);
    assert!(
        timeout(Duration::from_millis(100), reader.next())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn control_frames_overtake_queued_normal_frames() {
    let (client, server) = duplex(64);
    let (sender, _task) = spawn_writer(client);

    for tag in 1..=3 {
        sender
            .send(&frame(tag, 200), Priority::Normal)
            .await
            .unwrap();
    }
    sender.send(&frame(9, 0), Priority::Control).await.unwrap();

    let mut reader = FrameReader::new(server);
    let mut order = Vec::new();
    for _ in 0..4 {
        let received = reader.next().await.unwrap().unwrap();
        order.push(String::from_utf8(received.metadata).unwrap());
    }
    let position = order.iter().position(|m| m == "{\"t\":9}").unwrap();
    assert!(
        position <= 1,
        "control frame arrived at position {position}: {order:?}"
    );
}

#[tokio::test]
async fn writer_applies_backpressure_instead_of_queueing_without_bound() {
    let (client, _server_never_reads) = duplex(64);
    let (sender, _task) = spawn_writer(client);
    let big = frame(5, MAX_BODY_LEN);

    let mut accepted = 0;
    loop {
        match timeout(
            Duration::from_millis(200),
            sender.send(&big, Priority::Normal),
        )
        .await
        {
            Ok(result) => {
                result.unwrap();
                accepted += 1;
            }
            Err(_) => break,
        }
        assert!(accepted < 20, "queue grew without bound");
    }
    assert!(
        (1..=8).contains(&accepted),
        "accepted {accepted} frames of ~256 KiB"
    );
}

#[tokio::test]
async fn sending_after_the_connection_died_fails() {
    let (client, server) = duplex(64);
    let (sender, task) = spawn_writer(client);
    drop(server);

    let _ = sender.send(&frame(1, 100), Priority::Normal).await;
    let _ = task.await;
    assert!(sender.send(&frame(2, 1), Priority::Normal).await.is_err());
}
