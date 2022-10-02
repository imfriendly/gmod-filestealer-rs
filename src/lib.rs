use windows::{
    Win32::System::SystemServices::*,
    Win32::System::LibraryLoader::*,
    Win32::Foundation::*,
    core::{ w, s }
};

use std::ffi::{ c_char, CStr };
use std::path::{ Path, PathBuf };
use std::fs::{ OpenOptions, create_dir_all };
use std::io::Write;
use regex::Regex;
use detour::static_detour;
use std::sync::mpsc;
use std::mem::MaybeUninit;
use std::sync::mpsc::{ Sender, Receiver };
use std::thread;

struct LuaFileData {
    pub file_data: Vec<u8>,
    pub name: String
}

static mut DRIVE_LETTER: char = 'C';

// this shit is ugly
static mut TX: MaybeUninit<Sender<LuaFileData>> = MaybeUninit::uninit();
static mut RX: MaybeUninit<Receiver<LuaFileData>> = MaybeUninit::uninit();


const BAD_NAMES: [&str; 23] = ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9", ".."];

static_detour! {
    static LOADBUFFER: fn(*const u8, *const c_char, u64, *const c_char, *const u8) -> i32;
}

fn dump_file() {
    for received in unsafe { RX.assume_init_ref() } {
        let result = dump_file_impl(&received.file_data, &received.name);
        if let Err(e) = result {
            let path_string = format!("{}:/stealer/errors.txt", unsafe { DRIVE_LETTER });
    
            let path = Path::new(&path_string);
    
            create_dir_all(path.parent().unwrap()).unwrap();
    
            let mut f = OpenOptions::new()
                .read(false)
                .write(true)
                .create(true)
                .append(true)
                .open(path)
                .unwrap();
    
            f.write_all(e.as_bytes()).unwrap();
            f.write_all(b"\n").unwrap();
        }
    }
}

fn dump_file_impl(buffer_slice: &[u8], name_str: &str) -> Result<(), String> {
    let mut name_string = name_str.to_string();

    let filter = Regex::new(r"[^a-zA-Z0-9\./\\\-_\[\]!() ]").unwrap();
    name_string = filter.replace_all(name_string.as_str(), "").to_string(); // filtering weird characters that won't let ur files save

    if name_string.is_empty() {
        name_string = String::from("no_name.lua");
    }

    let mut unsanitised_path = format!("{}:/stealer/{}", unsafe { DRIVE_LETTER }, name_string);

    let extra_size_needed = "ex".len() + ".lua".len();
    unsanitised_path.truncate(MAX_PATH as usize - extra_size_needed);
    
    let unsanitised_path = Path::new(&unsanitised_path);

    let mut sanitised_path = PathBuf::new();
    for dir in unsanitised_path.iter() {
        let mut dir_str = dir.to_str()
            .unwrap()
            .to_owned();

        let dir_str_upper = dir_str.to_uppercase();
        for reserved_name in BAD_NAMES {
            if dir_str_upper == reserved_name {
                dir_str = String::from("ex");
            }       
        }
        sanitised_path.push(dir_str);
    }

    if sanitised_path.file_name().is_some() {
        sanitised_path.set_extension("lua");
    }
    
    create_dir_all(sanitised_path.parent().unwrap()).map_err(|e|e.to_string())?;

    let mut f = OpenOptions::new()
        .read(false)
        .write(true)
        .create(true)
        .append(true)
        .open(sanitised_path)
        .map_err(|e| e.to_string())?;
    
    f.write_all(buffer_slice).map_err(|e| e.to_string())?;
    f.write_all(b"\n\n").map_err(|e| e.to_string())?;

    Ok(())
}

fn send_data_to_thread(buffer: *const c_char, len: u64, name: *const c_char) -> Result<(), String> {
    let name_c_str = unsafe { CStr::from_ptr(name.add(1)) };
    let buffer_slice = unsafe { std::slice::from_raw_parts(buffer as *const u8, len as usize) };

    // creating copies to send to a different thread
    let buffer_data = buffer_slice.to_vec();    
    let name_string = name_c_str.to_str()
    .map_err(|e| e.to_string())?
    .to_owned();

    let data = LuaFileData { file_data: buffer_data, name: name_string };

    unsafe { TX.assume_init_ref().send(data)
        .map_err(|e| e.to_string())?; }

    Ok(())
}

#[allow(unused_unsafe)] // rust-analyser causes hooking crate to make red squiggly lines underneath :(
fn lua_loadbufferx_hook(state: *const u8, buffer: *const c_char, len: u64, name: *const c_char, mode: *const u8) -> i32 {
    let result = send_data_to_thread(buffer, len, name);
    if let Err(e) = result {
        let path_string = format!("{}:/stealer/errors.txt", unsafe { DRIVE_LETTER });
    
        let path = Path::new(&path_string);

        create_dir_all(path.parent().unwrap()).unwrap();

        let mut f = OpenOptions::new()
            .read(false)
            .write(true)
            .create(true)
            .append(true)
            .open(path)
            .unwrap();

        f.write_all(e.as_bytes()).unwrap();
        f.write_all(b"\n").unwrap();
    }
    
    return unsafe { LOADBUFFER.call(state, buffer, len, name, mode) };
}

fn initializer() {
    unsafe {
        let lua_shared = GetModuleHandleW(w!("lua_shared.dll")).unwrap();
        let lua_ptr = GetProcAddress(lua_shared, s!("luaL_loadbufferx")).unwrap() as *const ();
        let lua_func: fn(*const u8, *const c_char, u64, *const c_char, *const u8) -> i32 = std::mem::transmute(lua_ptr);
        let (tmptx, tmprx) = mpsc::channel::<LuaFileData>();

        TX.write(tmptx);
        RX.write(tmprx);

        let dir = std::env::current_exe().unwrap()
            .to_str()
            .unwrap()
            .to_owned();
      
        DRIVE_LETTER = dir.chars()
            .next()
            .unwrap(); // getting drive letter from game path

        LOADBUFFER.initialize(lua_func, lua_loadbufferx_hook).unwrap();
        
        LOADBUFFER.enable().unwrap();

        thread::spawn(dump_file);
    }
}

#[no_mangle]
extern "C" fn DllMain(_module: *const u8, reason: u32, _reserve: *const u8) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
       initializer();
    }

    1
}