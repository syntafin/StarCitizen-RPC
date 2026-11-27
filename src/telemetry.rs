use bytes::{Buf, BufMut, Bytes, BytesMut};
use prost::Message;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;

const TELEMETRY_PORTS: &[u16] = &[2942, 50051];
const GRPC_PATH: &str = "/sc.game.telemetry.v1.GameTelemetryService/TelemetryStream";
const STALE_TIMEOUT: Duration = Duration::from_secs(5);

// --- Protobuf-Schema (sc.game.telemetry.v1) ---

#[derive(Clone, PartialEq, Message)]
pub struct TelemetryStreamResponse {
    #[prost(message, optional, tag = "3")]
    pub vehicle: Option<VehicleFrame>,
    #[prost(message, optional, tag = "5")]
    pub events: Option<EventsFrame>,
    #[prost(message, optional, tag = "6")]
    pub actor: Option<ActorFrame>,
}

#[derive(Clone, PartialEq, Message)]
pub struct VehicleFrame {
    #[prost(message, optional, tag = "2")]
    pub state: Option<VehicleState>,
}

#[derive(Clone, PartialEq, Message)]
pub struct VehicleState {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub class_name: String,
    #[prost(string, tag = "3")]
    pub manufacturer: String,
    #[prost(int32, tag = "4")]
    pub r#type: i32,
    #[prost(message, optional, tag = "7")]
    pub annunciators: Option<VehicleAnnunciator>,
    #[prost(message, optional, tag = "8")]
    pub flight: Option<VehicleFlight>,
    #[prost(message, optional, tag = "10")]
    pub shields: Option<VehicleShields>,
    #[prost(float, tag = "11")]
    pub health: f32,
    #[prost(message, optional, tag = "12")]
    pub master_mode: Option<VehicleMasterModeState>,
}

#[derive(Clone, PartialEq, Message)]
pub struct VehicleAnnunciator {
    #[prost(int32, tag = "6")]
    pub qdrv: i32,
    #[prost(int32, tag = "9")]
    pub hit: i32,
    #[prost(int32, tag = "10")]
    pub misl: i32,
}

#[derive(Clone, PartialEq, Message)]
pub struct VehicleFlight {
    #[prost(message, optional, tag = "1")]
    pub state: Option<VehicleFlightState>,
}

#[derive(Clone, PartialEq, Message)]
pub struct VehicleFlightState {
    #[prost(bool, tag = "1")]
    pub is_landed: bool,
    #[prost(int32, optional, tag = "2")]
    pub gear: Option<i32>,
    #[prost(int32, optional, tag = "3")]
    pub vtol: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct VehicleShields {
    #[prost(float, tag = "1")]
    pub level: f32,
}

#[derive(Clone, PartialEq, Message)]
pub struct VehicleMasterModeState {
    #[prost(int32, tag = "1")]
    pub mode: i32,
    #[prost(bool, tag = "2")]
    pub spooled: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct EventsFrame {
    #[prost(message, repeated, tag = "1")]
    pub events: Vec<GameEvent>,
}

#[derive(Clone, PartialEq, Message)]
pub struct GameEvent {
    #[prost(int32, tag = "1")]
    pub r#type: i32,
}

#[derive(Clone, PartialEq, Message)]
pub struct ActorFrame {
    #[prost(message, optional, tag = "1")]
    pub vitals: Option<ActorVitals>,
    #[prost(message, optional, tag = "2")]
    pub state: Option<ActorState>,
    #[prost(message, optional, tag = "3")]
    pub environment: Option<ActorEnvironment>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Gauge {
    #[prost(float, tag = "1")]
    pub current: f32,
    #[prost(float, tag = "2")]
    pub max: f32,
}

#[derive(Clone, PartialEq, Message)]
pub struct ActorVitals {
    #[prost(message, optional, tag = "1")]
    pub health: Option<Gauge>,
    #[prost(message, optional, tag = "2")]
    pub oxygen: Option<Gauge>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ActorState {
    #[prost(int32, tag = "1")]
    pub locomotion: i32,
    #[prost(int32, tag = "2")]
    pub crew_role: i32,
    #[prost(string, tag = "3")]
    pub seat_name: String,
    #[prost(int64, tag = "4")]
    pub uec_balance: i64,
}

#[derive(Clone, PartialEq, Message)]
pub struct ActorEnvironment {
    #[prost(float, tag = "1")]
    pub external_temperature: f32,
    #[prost(float, tag = "2")]
    pub ambient_radiation_rate: f32,
    #[prost(bool, tag = "3")]
    pub breathable_atmosphere: bool,
}

// --- Normalisierte Domänen-Typen ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LocomotionState {
    #[default]
    Unknown,
    OnFoot,
    Eva,
    Seated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CrewRole {
    #[default]
    Unknown,
    Pilot,
    Gunner,
    Crew,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MasterMode {
    #[default]
    Unknown,
    Nav,
    Scm,
    Stealth,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VehicleTelemetry {
    pub instance_name: String,
    pub class_name: String,
    pub manufacturer: String,
    pub is_landed: bool,
    pub master_mode: MasterMode,
    pub quantum_spooled: bool,
    pub health: f32,
    pub shields_level: Option<f32>,
    pub under_fire: bool,
    pub missile_warning: bool,
}

#[derive(Debug, Clone)]
pub struct TelemetrySnapshot {
    pub updated_at: Instant,
    pub locomotion: LocomotionState,
    pub crew_role: CrewRole,
    pub uec_balance: Option<i64>,
    pub is_downed: bool,
    pub low_health: bool,
    pub low_oxygen: bool,
    pub breathable_atmosphere: bool,
    pub external_temperature: Option<f32>,
    pub ambient_radiation: Option<f32>,
    pub vehicle: Option<VehicleTelemetry>,
}

impl Default for TelemetrySnapshot {
    fn default() -> Self {
        Self {
            updated_at: Instant::now(),
            locomotion: LocomotionState::Unknown,
            crew_role: CrewRole::Unknown,
            uec_balance: None,
            is_downed: false,
            low_health: false,
            low_oxygen: false,
            breathable_atmosphere: true,
            external_temperature: None,
            ambient_radiation: None,
            vehicle: None,
        }
    }
}

impl TelemetrySnapshot {
    pub fn is_fresh(&self) -> bool {
        self.updated_at.elapsed() <= STALE_TIMEOUT
    }

    pub fn apply_frame(&mut self, frame: &TelemetryStreamResponse) {
        self.updated_at = Instant::now();

        if let Some(events_frame) = &frame.events {
            for ev in &events_frame.events {
                match ev.r#type {
                    2 | 6 => self.is_downed = true,     // DEAD / DOWNED
                    1 | 5 => self.is_downed = false,    // SPAWN / RESPAWN
                    _ => {}
                }
            }
        }

        if let Some(actor) = &frame.actor {
            if let Some(state) = &actor.state {
                self.locomotion = match state.locomotion {
                    1 => LocomotionState::OnFoot,
                    2 => LocomotionState::Eva,
                    3 => LocomotionState::Seated,
                    _ => LocomotionState::Unknown,
                };
                self.crew_role = match state.crew_role {
                    1 => CrewRole::Pilot,
                    2 => CrewRole::Gunner,
                    3 => CrewRole::Crew,
                    _ => CrewRole::Unknown,
                };
                if state.uec_balance > 0 {
                    self.uec_balance = Some(state.uec_balance);
                }
            }

            if let Some(vitals) = &actor.vitals {
                if let Some(hp) = &vitals.health {
                    if hp.max > 0.0 {
                        let ratio = hp.current / hp.max;
                        self.low_health = ratio <= 0.25;
                        if hp.current <= 0.0 {
                            self.is_downed = true;
                        } else if ratio > 0.5 && self.is_downed {
                            // Sicherheits-Reset, falls Respawn-Event verpasst wurde
                            self.is_downed = false;
                        }
                    }
                }
                if let Some(o2) = &vitals.oxygen {
                    if o2.max > 0.0 {
                        self.low_oxygen = (o2.current / o2.max) <= 0.20;
                    }
                }
            }

            if let Some(env) = &actor.environment {
                self.breathable_atmosphere = env.breathable_atmosphere;
                self.external_temperature = Some(env.external_temperature);
                self.ambient_radiation = Some(env.ambient_radiation_rate);
            }
        }

        if self.locomotion == LocomotionState::Seated {
            if let Some(v_frame) = &frame.vehicle {
                if let Some(vs) = &v_frame.state {
                    let is_landed = vs
                        .flight
                        .as_ref()
                        .and_then(|f| f.state.as_ref())
                        .map(|s| s.is_landed)
                        .unwrap_or(false);
                    let (master_mode, quantum_spooled) = vs
                        .master_mode
                        .as_ref()
                        .map(|mm| {
                            let mode = match mm.mode {
                                1 => MasterMode::Nav,
                                2 => MasterMode::Scm,
                                3 => MasterMode::Stealth,
                                _ => MasterMode::Unknown,
                            };
                            (mode, mm.spooled)
                        })
                        .unwrap_or((MasterMode::Unknown, false));

                    let (under_fire, missile_warning) = vs
                        .annunciators
                        .as_ref()
                        .map(|a| (a.hit >= 3, a.misl >= 3))
                        .unwrap_or((false, false));

                    self.vehicle = Some(VehicleTelemetry {
                        instance_name: vs.name.clone(),
                        class_name: vs.class_name.clone(),
                        manufacturer: vs.manufacturer.clone(),
                        is_landed,
                        master_mode,
                        quantum_spooled,
                        health: vs.health,
                        shields_level: vs.shields.as_ref().map(|s| s.level),
                        under_fire,
                        missile_warning,
                    });
                }
            }
        } else {
            self.vehicle = None;
        }
    }
}

// --- Hintergrund-Client ---

pub struct TelemetryClient {
    enabled: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    snapshot: Arc<Mutex<Option<TelemetrySnapshot>>>,
    handle: Option<JoinHandle<()>>,
}

impl TelemetryClient {
    pub fn start(initially_enabled: bool) -> Self {
        let enabled = Arc::new(AtomicBool::new(initially_enabled));
        let shutdown = Arc::new(AtomicBool::new(false));
        let snapshot = Arc::new(Mutex::new(None));

        let c_enabled = Arc::clone(&enabled);
        let c_shutdown = Arc::clone(&shutdown);
        let c_snapshot = Arc::clone(&snapshot);

        let handle = thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            rt.block_on(run_telemetry_loop(c_enabled, c_shutdown, c_snapshot));
        });

        Self {
            enabled,
            shutdown,
            snapshot,
            handle: Some(handle),
        }
    }

    pub fn set_enabled(&self, val: bool) {
        self.enabled.store(val, Ordering::SeqCst);
        if !val {
            if let Ok(mut guard) = self.snapshot.lock() {
                *guard = None;
            }
        }
    }

    pub fn latest(&self) -> Option<TelemetrySnapshot> {
        let guard = self.snapshot.lock().ok()?;
        guard.as_ref().filter(|s| s.is_fresh()).cloned()
    }
}

impl Drop for TelemetryClient {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

async fn run_telemetry_loop(
    enabled: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    snapshot: Arc<Mutex<Option<TelemetrySnapshot>>>,
) {
    let mut port_idx = 0usize;

    while !shutdown.load(Ordering::SeqCst) {
        if !enabled.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        }

        let port = TELEMETRY_PORTS[port_idx % TELEMETRY_PORTS.len()];
        port_idx = port_idx.wrapping_add(1);

        let _ = stream_from_port(port, &enabled, &shutdown, &snapshot).await;

        if !shutdown.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}

async fn stream_from_port(
    port: u16,
    enabled: &AtomicBool,
    shutdown: &AtomicBool,
    snapshot: &Mutex<Option<TelemetrySnapshot>>,
) -> Result<(), ()> {
    let addr = format!("127.0.0.1:{port}");
    let tcp = tokio::time::timeout(Duration::from_millis(800), TcpStream::connect(&addr))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())?;
    let _ = tcp.set_nodelay(true);

    let (mut client, h2_conn) = h2::client::handshake(tcp).await.map_err(|_| ())?;
    tokio::spawn(async move {
        let _ = h2_conn.await;
    });

    let request = http::Request::builder()
        .method(http::Method::POST)
        .uri(format!("http://127.0.0.1:{port}{GRPC_PATH}"))
        .header("content-type", "application/grpc")
        .header("te", "trailers")
        .body(())
        .map_err(|_| ())?;

    let (response_fut, mut send_stream) = client.send_request(request, false).map_err(|_| ())?;
    let empty_frame = Bytes::from_static(&[0, 0, 0, 0, 0]);
    send_stream.send_data(empty_frame, true).map_err(|_| ())?;

    let response = tokio::time::timeout(Duration::from_secs(3), response_fut)
        .await
        .map_err(|_| ())?
        .map_err(|_| ())?;

    if !response.status().is_success() {
        return Err(());
    }

    let mut body = response.into_body();
    let mut buf = BytesMut::with_capacity(4096);
    let mut local_state = TelemetrySnapshot::default();

    while !shutdown.load(Ordering::SeqCst) && enabled.load(Ordering::SeqCst) {
        let next_chunk = tokio::time::timeout(Duration::from_secs(3), body.data()).await;
        let chunk = match next_chunk {
            Ok(Some(Ok(bytes))) => bytes,
            _ => break,
        };
        let _ = body.flow_control().release_capacity(chunk.len());
        buf.put_slice(&chunk);

        while buf.len() >= 5 {
            let compressed = buf[0];
            let msg_len = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
            if buf.len() < 5 + msg_len {
                break;
            }
            buf.advance(5);
            let payload = buf.split_to(msg_len);
            if compressed == 0 {
                if let Ok(frame) = TelemetryStreamResponse::decode(payload.as_ref()) {
                    local_state.apply_frame(&frame);
                    if let Ok(mut guard) = snapshot.lock() {
                        *guard = Some(local_state.clone());
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_and_applies_actor_and_vehicle_frame() {
        let frame = TelemetryStreamResponse {
            vehicle: Some(VehicleFrame {
                state: Some(VehicleState {
                    name: "AEGS_Gladius_12345".to_string(),
                    class_name: "AEGS_Gladius".to_string(),
                    manufacturer: "AEGS".to_string(),
                    r#type: 1,
                    annunciators: Some(VehicleAnnunciator {
                        qdrv: 1,
                        hit: 4,
                        misl: 1,
                    }),
                    flight: Some(VehicleFlight {
                        state: Some(VehicleFlightState {
                            is_landed: false,
                            gear: Some(1),
                            vtol: Some(1),
                        }),
                    }),
                    shields: Some(VehicleShields { level: 0.72 }),
                    health: 0.95,
                    master_mode: Some(VehicleMasterModeState {
                        mode: 1,
                        spooled: true,
                    }),
                }),
            }),
            events: None,
            actor: Some(ActorFrame {
                vitals: Some(ActorVitals {
                    health: Some(Gauge {
                        current: 100.0,
                        max: 100.0,
                    }),
                    oxygen: Some(Gauge {
                        current: 95.0,
                        max: 100.0,
                    }),
                }),
                state: Some(ActorState {
                    locomotion: 3, // Seated
                    crew_role: 1,  // Pilot
                    seat_name: "PilotSeat".to_string(),
                    uec_balance: 4_200_000,
                }),
                environment: None,
            }),
        };

        let encoded = frame.encode_to_vec();
        let decoded = TelemetryStreamResponse::decode(encoded.as_slice()).expect("decode works");

        let mut snap = TelemetrySnapshot::default();
        snap.apply_frame(&decoded);

        assert_eq!(snap.locomotion, LocomotionState::Seated);
        assert_eq!(snap.crew_role, CrewRole::Pilot);
        assert_eq!(snap.uec_balance, Some(4_200_000));
        let v = snap.vehicle.expect("vehicle present");
        assert_eq!(v.class_name, "AEGS_Gladius");
        assert_eq!(v.master_mode, MasterMode::Nav);
        assert!(v.quantum_spooled);
        assert!(v.under_fire);
        assert!(!v.missile_warning);
    }

    #[test]
    fn tracks_downed_and_respawn_events() {
        let mut snap = TelemetrySnapshot::default();
        snap.apply_frame(&TelemetryStreamResponse {
            vehicle: None,
            events: Some(EventsFrame {
                events: vec![GameEvent { r#type: 6 }], // DOWNED
            }),
            actor: None,
        });
        assert!(snap.is_downed);

        snap.apply_frame(&TelemetryStreamResponse {
            vehicle: None,
            events: Some(EventsFrame {
                events: vec![GameEvent { r#type: 5 }], // RESPAWN
            }),
            actor: None,
        });
        assert!(!snap.is_downed);
    }
}
