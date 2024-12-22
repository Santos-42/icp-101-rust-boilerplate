#[macro_use]
extern crate serde;
use candid::{Decode, Encode};
use ic_cdk::api::time;
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::{BoundedStorable, Cell, DefaultMemoryImpl, StableBTreeMap, Storable};
use std::{borrow::Cow, cell::RefCell};

type Memory = VirtualMemory<DefaultMemoryImpl>;
type IdCell = Cell<u64, Memory>;

// Struct untuk data game
#[derive(candid::CandidType, Clone, Serialize, Deserialize)]
struct Game {
    id: u64,
    name: String,
    nominal: Vec<u64>,    // Nominal top-up yang tersedia (mis: 50, 100, 200)
    harga: Vec<u64>,      // Harga untuk masing-masing nominal
}

// Struct untuk transaksi top-up
#[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)]
struct TopUp {
    id: u64,
    game_id: u64,
    user_id: String,
    nominal: u64,
    harga: u64,
    status: StatusTransaksi,
    created_at: u64,
}

// Status untuk transaksi
#[derive(candid::CandidType, Clone, Serialize, Deserialize, PartialEq)]
enum StatusTransaksi {
    Menunggu,
    Berhasil,
    Gagal
}

impl Default for StatusTransaksi {
    fn default() -> Self {
        StatusTransaksi::Menunggu
    }
}

// Implementasi Storable untuk Game
impl Storable for Game {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }
    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

// Implementasi Storable untuk TopUp
impl Storable for TopUp {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }
    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

// Implementasi BoundedStorable
impl BoundedStorable for Game {
    const MAX_SIZE: u32 = 1024;
    const IS_FIXED_SIZE: bool = false;
}

impl BoundedStorable for TopUp {
    const MAX_SIZE: u32 = 1024;
    const IS_FIXED_SIZE: bool = false;
}

thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(
        MemoryManager::init(DefaultMemoryImpl::default())
    );

    static ID_COUNTER: RefCell<IdCell> = RefCell::new(
        IdCell::init(MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(0))), 0)
            .expect("Cannot create a counter")
    );

    static GAMES: RefCell<StableBTreeMap<u64, Game, Memory>> = RefCell::new(
        StableBTreeMap::init(MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(1))))
    );

    static TOPUPS: RefCell<StableBTreeMap<u64, TopUp, Memory>> = RefCell::new(
        StableBTreeMap::init(MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(2))))
    );
}

// Struct untuk input data
#[derive(candid::CandidType, Serialize, Deserialize)]
struct GamePayload {
    name: String,
    nominal: Vec<u64>,
    harga: Vec<u64>,
}

#[derive(candid::CandidType, Serialize, Deserialize)]
struct TopUpPayload {
    game_id: u64,
    user_id: String,
    nominal: u64,
}

// Error handling
#[derive(candid::CandidType, Deserialize, Serialize)]
enum Error {
    NotFound { msg: String },
    InvalidInput { msg: String },
}

// === FUNGSI QUERY ===

// Mendapatkan data game
#[ic_cdk::query]
fn get_game(id: u64) -> Result<Game, Error> {
    match GAMES.with(|service| service.borrow().get(&id)) {
        Some(game) => Ok(game),
        None => Err(Error::NotFound {
            msg: format!("Game dengan id={} tidak ditemukan", id),
        }),
    }
}

// Mendapatkan semua game
#[ic_cdk::query]
fn get_all_games() -> Vec<Game> {
    GAMES.with(|service| {
        service
            .borrow()
            .iter()
            .map(|(_, game)| game.clone())
            .collect()
    })
}

// Mendapatkan data top-up
#[ic_cdk::query]
fn get_topup(id: u64) -> Result<TopUp, Error> {
    match TOPUPS.with(|service| service.borrow().get(&id)) {
        Some(topup) => Ok(topup),
        None => Err(Error::NotFound {
            msg: format!("Top-up dengan id={} tidak ditemukan", id),
        }),
    }
}

// === FUNGSI UPDATE ===

// Menambah game baru
#[ic_cdk::update]
fn add_game(payload: GamePayload) -> Result<Game, Error> {
    // Validasi input
    if payload.nominal.len() != payload.harga.len() {
        return Err(Error::InvalidInput {
            msg: String::from("Jumlah nominal dan harga harus sama"),
        });
    }

    let id = ID_COUNTER.with(|counter| {
        let current_value = *counter.borrow().get();
        counter.borrow_mut().set(current_value + 1)
    })
    .expect("cannot increment id counter");

    let game = Game {
        id,
        name: payload.name,
        nominal: payload.nominal,
        harga: payload.harga,
    };

    GAMES.with(|service| service.borrow_mut().insert(id, game.clone()));
    Ok(game)
}

// Membuat transaksi top-up
#[ic_cdk::update]
fn create_topup(payload: TopUpPayload) -> Result<TopUp, Error> {
    // Cek game exists
    let game = match GAMES.with(|service| service.borrow().get(&payload.game_id)) {
        Some(game) => game,
        None => {
            return Err(Error::NotFound {
                msg: format!("Game dengan id={} tidak ditemukan", payload.game_id),
            })
        }
    };

    // Cek nominal valid
    let nominal_index = match game.nominal.iter().position(|&x| x == payload.nominal) {
        Some(index) => index,
        None => {
            return Err(Error::InvalidInput {
                msg: format!("Nominal {} tidak tersedia untuk game ini", payload.nominal),
            })
        }
    };

    let id = ID_COUNTER.with(|counter| {
        let current_value = *counter.borrow().get();
        counter.borrow_mut().set(current_value + 1)
    })
    .expect("cannot increment id counter");

    let topup = TopUp {
        id,
        game_id: payload.game_id,
        user_id: payload.user_id,
        nominal: payload.nominal,
        harga: game.harga[nominal_index],
        status: StatusTransaksi::Menunggu,
        created_at: time(),
    };

    TOPUPS.with(|service| service.borrow_mut().insert(id, topup.clone()));
    Ok(topup)
}

// Update status transaksi
#[ic_cdk::update]
fn update_status(id: u64, status: StatusTransaksi) -> Result<TopUp, Error> {
    TOPUPS.with(|service| {
        let mut storage = service.borrow_mut();
        match storage.get(&id) {
            Some(mut topup) => {
                topup.status = status;
                storage.insert(id, topup.clone());
                Ok(topup)
            }
            None => Err(Error::NotFound {
                msg: format!("Top-up dengan id={} tidak ditemukan", id),
            }),
        }
    })
}

// Export candid
ic_cdk::export_candid!();