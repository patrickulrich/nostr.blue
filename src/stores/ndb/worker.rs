use super::commands::{NdbEvent, NdbRequest, NoteData, ProfileData};
use crate::stores::ndb::get_ndb;
use nostrdb::{NoteKey, Transaction};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Mutex, OnceLock};

static CMD_CHANNEL: OnceLock<Sender<NdbRequest>> = OnceLock::new();
static WAKE_RX: OnceLock<Mutex<Option<Receiver<()>>>> = OnceLock::new();

static NDB_EVENT_CHANNELS: OnceLock<(
    tokio::sync::mpsc::UnboundedSender<NdbEvent>,
    Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<NdbEvent>>>,
)> = OnceLock::new();

pub fn set_wake_receiver(rx: Receiver<()>) {
    let _ = WAKE_RX.set(Mutex::new(Some(rx)));
}

pub fn ndb_event_sender() -> Option<&'static tokio::sync::mpsc::UnboundedSender<NdbEvent>> {
    NDB_EVENT_CHANNELS.get().map(|(tx, _)| tx)
}

pub fn take_event_receiver() -> Option<tokio::sync::mpsc::UnboundedReceiver<NdbEvent>> {
    NDB_EVENT_CHANNELS
        .get()
        .and_then(|(_, rx_mutex)| rx_mutex.lock().unwrap_or_else(|e| e.into_inner()).take())
}

pub fn start_ndb_worker() -> Result<(), String> {
    let (cmd_tx, cmd_rx): (Sender<NdbRequest>, Receiver<NdbRequest>) =
        std::sync::mpsc::channel();
    let _ = CMD_CHANNEL.set(cmd_tx);

    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
    let _ = NDB_EVENT_CHANNELS.set((event_tx, Mutex::new(Some(event_rx))));

    std::thread::Builder::new()
        .name("nostrdb-worker".into())
        .spawn(move || worker_loop(cmd_rx))
        .map_err(|e| format!("Failed to spawn nostrdb worker: {}", e))?;

    log::info!("NdbWorker started on dedicated thread");
    Ok(())
}

pub fn stop_ndb_worker() {
    if let Some(tx) = CMD_CHANNEL.get() {
        let _ = tx.send(NdbRequest::Shutdown);
    }
}

pub fn send_request(req: NdbRequest) -> Result<(), String> {
    let tx = CMD_CHANNEL.get().ok_or("NdbWorker not running")?;
    tx.send(req).map_err(|e| format!("NdbWorker send failed: {}", e))
}

fn parse_filters(jsons: &[String]) -> Vec<nostrdb::Filter> {
    jsons
        .iter()
        .filter_map(|j| nostrdb::Filter::from_json(j).ok())
        .collect()
}

fn worker_loop(cmd_rx: Receiver<NdbRequest>) {
    let mut subscriptions: HashMap<String, nostrdb::Subscription> = HashMap::new();
    let mut initialized = false;

    loop {
        if !initialized {
            if get_ndb().is_some() {
                initialized = true;
            } else {
                std::thread::sleep(std::time::Duration::from_millis(100));
                continue;
            }
        }

        while let Ok(req) = cmd_rx.try_recv() {
            match req {
                NdbRequest::Shutdown => {
                    log::info!("NdbWorker shutting down");
                    return;
                }
                NdbRequest::QueryNoteKeys {
                    filter_jsons,
                    limit,
                    reply,
                } => {
                    let filters = parse_filters(&filter_jsons);
                    let _ = reply.send(query_note_keys_sync(&filters, limit));
                }
                NdbRequest::GetNoteData { key, reply } => {
                    let _ = reply.send(get_note_data_by_key_sync(key));
                }
                NdbRequest::GetNoteDataById { id, reply } => {
                    let _ = reply.send(get_note_data_by_id_sync(&id));
                }
                NdbRequest::GetProfile { pubkey, reply } => {
                    let _ = reply.send(get_profile_sync(&pubkey));
                }
                NdbRequest::Subscribe {
                    key,
                    filter_jsons,
                    reply,
                } => {
                    let filters = parse_filters(&filter_jsons);
                    let result = subscribe_sync(&key, &filters, &mut subscriptions);
                    let _ = reply.send(result);
                }
                NdbRequest::Unsubscribe { key } => {
                    if let Some(sub) = subscriptions.remove(&key) {
                        if let Some(ndb) = get_ndb() {
                            let mut ndb_inner = (*ndb).clone();
                            let _ = ndb_inner.unsubscribe(sub);
                        }
                    }
                }
            }
        }

        let woken = WAKE_RX
            .get()
            .map(|rx_mutex| {
                rx_mutex
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|rx| rx.try_recv() == Ok(()))
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        if woken {
            poll_all_subscriptions(&subscriptions);
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn query_note_keys_sync(
    filters: &[nostrdb::Filter],
    limit: i32,
) -> Result<Vec<(NoteKey, u64)>, String> {
    let ndb = get_ndb().ok_or("NDB not initialized")?;
    let txn = Transaction::new(ndb).map_err(|e| format!("Transaction: {}", e))?;
    let results = ndb
        .query(&txn, filters, limit)
        .map_err(|e| format!("Query: {}", e))?;
    Ok(results
        .into_iter()
        .map(|r| (r.note_key, r.note.created_at()))
        .collect())
}

fn get_note_data_by_key_sync(key: NoteKey) -> Result<NoteData, String> {
    let ndb = get_ndb().ok_or("NDB not initialized")?;
    let txn = Transaction::new(ndb).map_err(|e| format!("Transaction: {}", e))?;
    let note = ndb
        .get_note_by_key(&txn, key)
        .map_err(|e| format!("Get note: {}", e))?;
    Ok(note_to_note_data(&note))
}

fn get_note_data_by_id_sync(id: &[u8; 32]) -> Result<Option<NoteData>, String> {
    let ndb = get_ndb().ok_or("NDB not initialized")?;
    let txn = Transaction::new(ndb).map_err(|e| format!("Transaction: {}", e))?;
    let result = match ndb.get_note_by_id(&txn, id) {
        Ok(note) => Ok(Some(note_to_note_data(&note))),
        Err(_) => Ok(None),
    };
    drop(txn);
    result
}

fn get_profile_sync(pubkey: &[u8; 32]) -> Result<Option<ProfileData>, String> {
    let ndb = get_ndb().ok_or("NDB not initialized")?;
    let txn = Transaction::new(ndb).map_err(|e| format!("Transaction: {}", e))?;
    match ndb.get_profile_by_pubkey(&txn, pubkey) {
        Ok(record) => {
            let profile = record.record().profile();
            let data = ProfileData {
                name: profile.and_then(|p| p.name().map(|s| s.to_string())),
                display_name: profile.and_then(|p| p.display_name().map(|s| s.to_string())),
                about: profile.and_then(|p| p.about().map(|s| s.to_string())),
                picture: profile.and_then(|p| p.picture().map(|s| s.to_string())),
                banner: profile.and_then(|p| p.banner().map(|s| s.to_string())),
                nip05: profile.and_then(|p| p.nip05().map(|s| s.to_string())),
                website: profile.and_then(|p| p.website().map(|s| s.to_string())),
                lud16: profile.and_then(|p| p.lud16().map(|s| s.to_string())),
            };
            drop(txn);
            Ok(Some(data))
        }
        Err(_) => Ok(None),
    }
}

fn subscribe_sync(
    key: &str,
    filters: &[nostrdb::Filter],
    subscriptions: &mut HashMap<String, nostrdb::Subscription>,
) -> Result<(), String> {
    let ndb = get_ndb().ok_or("NDB not initialized")?;
    if subscriptions.contains_key(key) {
        return Ok(());
    }
    let sub = ndb
        .subscribe(filters)
        .map_err(|e| format!("Subscribe: {}", e))?;
    subscriptions.insert(key.to_string(), sub);
    Ok(())
}

fn poll_all_subscriptions(subscriptions: &HashMap<String, nostrdb::Subscription>) {
    let ndb = match get_ndb() {
        Some(n) => n,
        None => return,
    };
    let sender = match ndb_event_sender() {
        Some(s) => s,
        None => return,
    };

    for (key, sub) in subscriptions {
        let new_keys = ndb.poll_for_notes(*sub, 500);
        if !new_keys.is_empty() {
            let new_notes: Vec<NoteData> = match Transaction::new(ndb) {
                Ok(txn) => new_keys
                    .iter()
                    .filter_map(|k| ndb.get_note_by_key(&txn, *k).ok())
                    .map(|note| note_to_note_data(&note))
                    .collect(),
                Err(_) => Vec::new(),
            };
            if !new_notes.is_empty() {
                let _ = sender.send(NdbEvent::SubscriptionUpdated {
                    key: key.clone(),
                    new_notes,
                });
            }
        }
    }
}

fn note_to_note_data(note: &nostrdb::Note) -> NoteData {
    NoteData {
        id: *note.id(),
        pubkey: *note.pubkey(),
        kind: note.kind() as u16,
        content: note.content().to_string(),
        created_at: note.created_at(),
        tags: extract_tags(note),
        sig: note.sig().to_vec(),
    }
}

fn extract_tags(note: &nostrdb::Note) -> Vec<Vec<String>> {
    let tags = note.tags();
    let mut result = Vec::with_capacity(tags.count() as usize);
    for tag in tags.iter() {
        let mut tag_vec = Vec::with_capacity(tag.count() as usize);
        for i in 0..tag.count() {
            if let Some(s) = tag.get_str(i) {
                tag_vec.push(s.to_string());
            } else if let Some(id) = tag.get_id(i) {
                tag_vec.push(hex::encode(id));
            }
        }
        result.push(tag_vec);
    }
    result
}
