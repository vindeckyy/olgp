//! voice-v1 WebSocket session handling.

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

use crate::telemetry::trace_event;

/// Deterministic speak gate. Only online-visible inputs are used.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GateInput {
    pub repeat_count: u32,
    pub answer_length: u32,
    pub copy_ratio: u32,
    pub asr_confidence: u32,
    pub position: u32,
}

/// Decide whether a tentative answer is stable enough to speak.
#[must_use]
pub fn should_speak(input: GateInput) -> bool {
    (input.repeat_count >= 2 && input.asr_confidence >= 80)
        || (input.repeat_count >= 1
            && (1..=8).contains(&input.answer_length)
            && input.copy_ratio <= 35
            && input.asr_confidence >= 90
            && input.position >= 35)
}

fn pcm_energy(frame: &[u8]) -> u64 {
    let samples = &frame[1..];
    if samples.is_empty() {
        return 0;
    }
    let sum: u64 = samples
        .iter()
        .map(|byte| {
            let sample = i64::from(*byte) - 128;
            (sample * sample) as u64
        })
        .sum();
    sum / samples.len() as u64
}

/// One voice session state.
#[derive(Debug, Default)]
pub struct VoiceSession {
    transcript: String,
    tentative: Option<String>,
}

impl VoiceSession {
    /// Record a partial transcript.
    pub fn partial(&mut self, text: &str) {
        self.transcript = text.to_owned();
    }

    /// Record a tentative answer.
    pub fn answer(&mut self, text: &str) {
        self.tentative = Some(text.to_owned());
    }

    /// Commit or withhold the tentative answer.
    #[must_use]
    pub fn decide(&mut self, input: GateInput) -> Option<String> {
        if self.tentative.is_some() && should_speak(input) {
            self.tentative.take()
        } else {
            None
        }
    }
}

fn text_frame(value: Value) -> Message {
    Message::Text(value.to_string().into())
}

/// Serve one voice WebSocket connection.
///
/// # Errors
/// Returns a WebSocket or IO error when the connection fails.
pub async fn serve(socket: TcpStream) -> Result<(), String> {
    let mut stream: WebSocketStream<TcpStream> = tokio_tungstenite::accept_async(socket)
        .await
        .map_err(|e| e.to_string())?;
    let ready = json!({
        "type": "ready",
        "protocolVersion": 1,
        "sessionId": "olgp-voice",
        "mode": "voice",
        "audio": {"format": "pcm_s16le", "sampleRate": 24000, "channels": 1}
    });
    stream.send(text_frame(ready)).await.map_err(|e| e.to_string())?;
    let mut session = VoiceSession::default();
    let mut speaking = false;
    let mut loud_frames = 0u32;
    while let Some(message) = stream.next().await {
        let message = message.map_err(|e| e.to_string())?;
        match message {
            Message::Text(text) => {
                let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
                match kind {
                    "text_input" | "stt_partial" => {
                        if let Some(text) = value.get("text").and_then(Value::as_str) {
                            session.partial(text);
                            let _ = trace_event("stt_partial");
                            stream
                                .send(text_frame(json!({"type":"stt_partial","text":text})))
                                .await
                                .map_err(|e| e.to_string())?;
                        }
                    }
                    "text_tentative" => {
                        if let Some(text) = value.get("text").and_then(Value::as_str) {
                            session.answer(text);
                            stream
                                .send(text_frame(json!({"type":"text_tentative","text":text})))
                                .await
                                .map_err(|e| e.to_string())?;
                        }
                    }
                    "commit" => {
                        if let Some(text) = session.decide(GateInput {
                            repeat_count: 2,
                            answer_length: 4,
                            copy_ratio: 10,
                            asr_confidence: 90,
                            position: 50,
                        }) {
                            stream
                                .send(text_frame(json!({"type":"text","text":text})))
                                .await
                                .map_err(|e| e.to_string())?;
                            stream
                                .send(text_frame(json!({"type":"end"})))
                                .await
                                .map_err(|e| e.to_string())?;
                            speaking = true;
                        }
                    }
                    "cancel" | "reset" => {
                        session = VoiceSession::default();
                        stream
                            .send(text_frame(json!({"type":"end"})))
                            .await
                            .map_err(|e| e.to_string())?;
                    }
                    _ => {}
                }
            }
            Message::Binary(frame) if frame.first() == Some(&0x01) => {
                let _ = trace_event("audio_pcm");
                if speaking {
                    if pcm_energy(&frame) > 10_000 {
                        loud_frames += 1;
                    } else {
                        loud_frames = 0;
                    }
                    if loud_frames >= 2 {
                        speaking = false;
                        loud_frames = 0;
                        session = VoiceSession::default();
                        stream
                            .send(text_frame(json!({"type":"cancel","reason":"barge_in"})))
                            .await
                            .map_err(|e| e.to_string())?;
                        stream
                            .send(text_frame(json!({"type":"input_committed"})))
                            .await
                            .map_err(|e| e.to_string())?;
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    Ok(())
}

/// Serve one speaker PCM stream. Frames are acknowledged, not forwarded to
/// physical hardware from this protocol repo.
///
/// # Errors
/// Returns a WebSocket or IO error when the connection fails.
pub async fn serve_speaker_audio(socket: TcpStream) -> Result<(), String> {
    let mut stream: WebSocketStream<TcpStream> = tokio_tungstenite::accept_async(socket)
        .await
        .map_err(|e| e.to_string())?;
    stream
        .send(text_frame(json!({
            "type": "ready",
            "protocolVersion": 1,
            "audio": {"format": "pcm_s16le", "channels": 1}
        })))
        .await
        .map_err(|e| e.to_string())?;
    while let Some(message) = stream.next().await {
        match message.map_err(|e| e.to_string())? {
            Message::Binary(frame) if frame.first() == Some(&0x01) => {
                let _ = trace_event("speaker_pcm");
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    stream
        .send(text_frame(json!({"type":"closed"})))
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn websocket_session_emits_events() {
        use tokio::net::{TcpListener, TcpStream};
        use tokio_tungstenite::client_async;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            serve(socket).await.unwrap();
        });
        let stream = TcpStream::connect(address).await.unwrap();
        let (mut client, _) = client_async("ws://127.0.0.1/realtime", stream)
            .await
            .unwrap();
        let ready = client.next().await.unwrap().unwrap().into_text().unwrap();
        assert!(ready.contains("\"type\":\"ready\""));

        client
            .send(Message::Text(
                json!({"type":"text_input","text":"question"}).to_string().into(),
            ))
            .await
            .unwrap();
        let partial = client.next().await.unwrap().unwrap().into_text().unwrap();
        assert!(partial.contains("stt_partial"));

        client
            .send(Message::Text(
                json!({"type":"text_tentative","text":"answer"}).to_string().into(),
            ))
            .await
            .unwrap();
        let tentative = client.next().await.unwrap().unwrap().into_text().unwrap();
        assert!(tentative.contains("text_tentative"));

        client
            .send(Message::Text(json!({"type":"commit"}).to_string().into()))
            .await
            .unwrap();
        let final_text = client.next().await.unwrap().unwrap().into_text().unwrap();
        assert!(final_text.contains("\"type\":\"text\""));
        let end = client.next().await.unwrap().unwrap().into_text().unwrap();
        assert!(end.contains("\"type\":\"end\""));

        client.close(None).await.unwrap();
        let _ = server.await;
    }

    #[test]
    fn strong_answer_fires() {
        assert!(should_speak(GateInput {
            repeat_count: 2,
            answer_length: 4,
            copy_ratio: 0,
            asr_confidence: 90,
            position: 10,
        }));
    }

    #[test]
    fn weak_answer_waits() {
        assert!(!should_speak(GateInput {
            repeat_count: 0,
            answer_length: 4,
            copy_ratio: 0,
            asr_confidence: 90,
            position: 50,
        }));
    }

    #[test]
    fn session_commits_once() {
        let mut session = VoiceSession::default();
        session.partial("question");
        session.answer("answer");
        assert_eq!(
            session.decide(GateInput {
                repeat_count: 2,
                answer_length: 6,
                copy_ratio: 0,
                asr_confidence: 90,
                position: 50,
            }),
            Some("answer".into())
        );
        assert_eq!(session.decide(GateInput {
            repeat_count: 2,
            answer_length: 6,
            copy_ratio: 0,
            asr_confidence: 90,
            position: 50,
        }), None);
    }

    #[test]
    fn short_high_confidence_answer_fires() {
        assert!(should_speak(GateInput {
            repeat_count: 1,
            answer_length: 4,
            copy_ratio: 35,
            asr_confidence: 90,
            position: 35,
        }));
    }

    #[tokio::test]
    async fn websocket_ignores_unknown_events_and_counts_pcm() {
        use tokio::net::{TcpListener, TcpStream};
        use tokio_tungstenite::client_async;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            serve(socket).await.unwrap();
        });
        let stream = TcpStream::connect(address).await.unwrap();
        let (mut client, _) = client_async("ws://127.0.0.1/realtime", stream).await.unwrap();
        assert!(client.next().await.unwrap().unwrap().into_text().unwrap().contains("ready"));

        client
            .send(Message::Text(json!({"type":"stt_final","text":"done"}).to_string().into()))
            .await
            .unwrap();
        client
            .send(Message::Binary(vec![0x01, 0, 0, 0].into()))
            .await
            .unwrap();
        client.send(Message::Text(json!({"type":"cancel"}).to_string().into())).await.unwrap();
        let end = client.next().await.unwrap().unwrap().into_text().unwrap();
        assert!(end.contains("\"type\":\"end\""));
        client.close(None).await.unwrap();
        let _ = server.await;
    }

    #[tokio::test]
    async fn websocket_detects_barge_in_during_speech() {
        use tokio::net::{TcpListener, TcpStream};
        use tokio_tungstenite::client_async;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            serve(socket).await.unwrap();
        });
        let stream = TcpStream::connect(address).await.unwrap();
        let (mut client, _) = client_async("ws://127.0.0.1/realtime", stream).await.unwrap();
        assert!(client.next().await.unwrap().unwrap().into_text().unwrap().contains("ready"));

        client
            .send(Message::Text(json!({"type":"text_tentative","text":"answer"}).to_string().into()))
            .await
            .unwrap();
        assert!(client.next().await.unwrap().unwrap().into_text().unwrap().contains("text_tentative"));
        client.send(Message::Text(json!({"type":"commit"}).to_string().into())).await.unwrap();
        assert!(client.next().await.unwrap().unwrap().into_text().unwrap().contains("\"type\":\"text\""));
        assert!(client.next().await.unwrap().unwrap().into_text().unwrap().contains("\"type\":\"end\""));

        let loud = vec![0x01, 0xff, 0xff, 0xff, 0xff];
        client.send(Message::Binary(loud.clone().into())).await.unwrap();
        client.send(Message::Binary(loud.into())).await.unwrap();
        let cancel = client.next().await.unwrap().unwrap().into_text().unwrap();
        assert!(cancel.contains("barge_in"));
        let committed = client.next().await.unwrap().unwrap().into_text().unwrap();
        assert!(committed.contains("input_committed"));

        client.close(None).await.unwrap();
        let _ = server.await;
    }
}
