#[macro_use]
extern crate bitflags;
extern crate byteorder;
extern crate rustc_serialize;
extern crate minifb;
extern crate clock_ticks;

use std::fs::File;
use std::io::Read;

use rustc_serialize::hex::ToHex;

use byteorder::{ByteOrder, LittleEndian};

#[derive(Debug)]
struct CH16Header {
    magic: String,
    reserved: u8,
    version: u8,
    size: u32,
    start: u16,
    crc32: u32,
}

impl<'a> From<&'a [u8]> for CH16Header {
    fn from(val: &[u8]) -> CH16Header {
        CH16Header {
            magic: String::from_utf8(val[..0x04].to_vec()).unwrap(),
            reserved: val[0x04],
            version: val[0x05],
            size: LittleEndian::read_u32(&val[0x06..0x0A]),
            start: LittleEndian::read_u16(&val[0x0A..0x0C]),
            crc32: LittleEndian::read_u32(&val[0x0C..0x10]),
        }
    }
}

#[allow(dead_code)]
bitflags! {
    flags Flags: u8 {
        const CLEAR     = 0b00000000,
        const CARRY     = 0b00000010,
        const ZERO      = 0b00000100,
        const OVERFLOW  = 0b01000000,
        const NEGATIVE  = 0b10000000,
    }
}

const STACK_START: u16 = 0xFDF0;
#[allow(dead_code)]
const IO_ADDR: u16 = 0xFFF0;
const MEMORY: usize = 0xFFFF;

#[allow(dead_code)]
struct CHIP16 {
    memory: [u8; MEMORY],

    pc: u16,
    sp: u16,
    regs: [i16; 16],

    flags: Flags,

    bg: Color,
    fg: Color,

    spritew: u8,
    spriteh: u8,

    vblank: bool,
}

#[allow(dead_code)]
#[derive(PartialEq)]
pub enum State {
    Continue,
    Stop,
}

#[derive(Clone)]
pub enum Color {
    Transparent,
    Black,
    Gray,
    Red,
    Pink,
    DarkBrown,
    Brown,
    Orange,
    Yelow,
    Green,
    LightGreen,
    DarkBlue,
    Blue,
    LightBlue,
    SkyBlue,
    White,
}

impl From<u8> for Color {
    fn from(val: u8) -> Color {
        match val {
            0xF => Color::White,
            _ => Color::Transparent,
        }
    }
}

impl Into<u32> for Color {
    fn into(self: Color) -> u32 {
        match self {
            Color::White => 0xFFFFFFFF,
            _ => 0x00000000,
        }
    }
}

impl CHIP16 {
    fn new(header: &CH16Header, cart: &[u8]) -> CHIP16 {
        let mut ret = CHIP16 {
            memory: [0; MEMORY],
            pc: header.start,
            sp: STACK_START,
            regs: [0; 16],
            flags: CLEAR,
            bg: Color::Transparent,
            fg: Color::Transparent,
            spritew: 0,
            spriteh: 0,
            vblank: false,
        };

        for i in 0..header.size {
            ret.memory[i as usize] = cart[i as usize];
        }

        ret
    }

    fn cycle(&mut self, screen: &Arc<Mutex<Vec<u32>>>) -> State {
        // print!("{:#X}: ", self.pc);
        let instr = &self.memory[self.pc as usize..self.pc as usize + 4];
        let opcode = instr[0];

        let ll: u16 = instr[2] as u16;
        let hh: u16 = instr[3] as u16;
        let hhll: u16 = hh << 8 | ll;
        let val = self.memory[hhll as usize];

        let x = instr[1] & 0x0F;
        let y = (instr[1] & 0xF0) >> 4;
        let z = instr[2] & 0x0F;

        let rx: i16 = self.regs[x as usize];
        let ry: i16 = self.regs[y as usize];

        self.pc += 4;
        match opcode {
            0x01 => {
                self.fg = Color::Transparent;
                self.bg = Color::Transparent;

                let mut buff = screen.lock().unwrap();

                for i in buff.iter_mut() {
                    *i = self.bg.clone().into();
                }

                // println!("CLS");
            }
            0x02 => {
                if !self.vblank {
                    self.pc -= 4;
                } else {
                    self.vblank = false;
                }
                // println!("VBLNK");
            }
            0x03 => {
                let c = instr[2] & 0x0F;

                self.bg = c.into();

                // println!("BGC {}", c);
            }
            0x04 => {
                let w = instr[2];
                let h = instr[3];

                self.spritew = w;
                self.spriteh = h;

                // println!("SPR w:{} h:{}", w, h);
            }
            0x05 => {
                let mut buff = screen.lock().unwrap();

                let mut xpos = rx as i16;
                let mut ypos = ry as i16;
                let mut addr = hhll as usize;

                // println!("DRW R{:X}, R{:X}, {:#X}", x, y, hhll);

                for j in 0..self.spriteh {
                    ypos += j as i16;
                    for i in 0..self.spritew {
                        let color = self.memory[addr];
                        let left: Color = ((color & 0xF0) >> 4 as u8).into();
                        let right: Color = ((color & 0x0F) as u8).into();
                        let pos = (xpos as i64 + ypos as i64 * WIDTH as i64) as usize;

                        buff[pos + 0] = left.into();
                        buff[pos + 1] = right.into();

                        addr += (i * j + self.spritew as u8) as usize;
                        xpos += i as i16 * 2;

                        // TODO: Check collision
                    }
                }
            }
            0x10 => {
                self.pc = hhll;

                // println!("JMP {:#X}", hhll);
            }
            0x12 => {
                match x {
                    0x00 => {
                        if (self.flags & ZERO) == ZERO {
                            self.pc = hhll;
                        }

                        // println!("JZ {:#X}", hhll)
                    }
                    0x09 => {
                        if (self.flags & CARRY) == CARRY {
                            self.pc = hhll;
                        }

                        // println!("JB {:#X}", hhll)
                    }
                    _ => panic!("J{:x} {:#X}", x, hhll),
                }
            }
            0x13 => {
                if rx == ry {
                    self.pc = hhll;
                }

                // println!("JME R{:X}, R{:X}, {:#X}", x, y, hhll);
            }
            0x20 => {
                self.regs[x as usize] = hhll as i16;

                // println!("LDI R{:X}, {:#X}", x, hhll);
            }
            0x24 => {
                self.regs[x as usize] = ry;

                // println!("MOV R{:X}, R{:X}", x, y);
            }
            0x41 => {
                let (res, of) = rx.overflowing_add(ry);

                self.regs[x as usize] = res;

                // println!("ADD R{:X}, R{:X}", x, y);

                if of {
                    self.flags |= CARRY;
                } else {
                    self.flags |= CARRY;
                }

                if (rx < 0 && ry < 0 && res >= 0) || (rx >= 0 && ry >= 0 && res < 0) {
                    self.flags |= OVERFLOW;
                } else {
                    self.flags &= !OVERFLOW;
                }

                if res == 0 {
                    self.flags |= ZERO;
                } else {
                    self.flags &= !ZERO;
                }

                if res < 0 {
                    self.flags |= NEGATIVE;
                } else {
                    self.flags &= !NEGATIVE;
                }
            }
            0x50 => {
                let (res, of) = rx.overflowing_sub(hhll as i16);
                self.regs[x as usize] = res;

                // println!("SUB R{:X}, {:#X}", x, hhll as i16);

                if rx < hhll as i16 {
                    self.flags |= CARRY;
                } else {
                    self.flags &= !CARRY;
                }

                if of {
                    self.flags |= OVERFLOW;
                } else {
                    self.flags &= !OVERFLOW;
                }

                if res < 0 {
                    self.flags |= NEGATIVE;
                } else {
                    self.flags &= !NEGATIVE;
                }

                if res == 0 {
                    self.flags |= ZERO;
                } else {
                    self.flags &= !ZERO;
                }
            }
            0x51 => {
                let (res, of) = rx.overflowing_sub(ry);
                self.regs[x as usize] = res;

                // println!("SUB R{:X}, R{:X}", x, y);

                if rx < ry {
                    self.flags |= CARRY;
                } else {
                    self.flags &= !CARRY;
                }

                if of {
                    self.flags |= OVERFLOW;
                } else {
                    self.flags &= !OVERFLOW;
                }

                if res < 0 {
                    self.flags |= NEGATIVE;
                } else {
                    self.flags &= !NEGATIVE;
                }

                if res == 0 {
                    self.flags |= ZERO;
                } else {
                    self.flags &= !ZERO;
                }
            }
            0x52 => {
                let (res, of) = rx.overflowing_sub(ry);
                self.regs[z as usize] = res;

                // println!("SUB R{:X}, R{:X}, R{:X}", x, y, z);

                if rx < ry {
                    self.flags |= CARRY;
                } else {
                    self.flags &= !CARRY;
                }

                if of {
                    self.flags |= OVERFLOW;
                } else {
                    self.flags &= !OVERFLOW;
                }

                if res < 0 {
                    self.flags |= NEGATIVE;
                } else {
                    self.flags &= !NEGATIVE;
                }

                if res == 0 {
                    self.flags |= ZERO;
                } else {
                    self.flags &= !ZERO;
                }
            }
            _ => {
                panic!("Unknown opcode: {:#x} instr: 0x{}",
                       opcode,
                       (*instr).to_hex().to_uppercase())
            }
        }

        State::Continue
    }
}

const WIDTH: usize = 320;
const HEIGHT: usize = 240;

use minifb::{Key, Scale, WindowOptions};
use std::thread;
use std::time::Duration;
use std::sync::{Arc, Mutex};

pub fn draw_loop<F>(rate: u64, mut callback: F)
    where F: FnMut() -> State
{
    let mut accumulator = 0;
    let mut previous_clock = clock_ticks::precise_time_ns();

    let rate = 1_000_000_000 / rate;

    loop {
        match callback() {
            State::Stop => break,
            State::Continue => (),
        };

        let now = clock_ticks::precise_time_ns();
        accumulator += now - previous_clock;
        previous_clock = now;

        while accumulator >= rate {
            accumulator -= rate;
        }

        thread::sleep(Duration::from_millis(((rate - accumulator) / 1000000) as u64));
    }
}

pub fn cpu_loop<F>(rate: u64, mut callback: F)
    where F: FnMut() -> State + Send + 'static
{
    thread::spawn(move || {
        let mut accumulator = 0;
        let mut previous_clock = clock_ticks::precise_time_ns();

        let rate = 1_000_000_000 / rate;

        loop {
            match callback() {
                State::Stop => break,
                State::Continue => (),
            };

            let now = clock_ticks::precise_time_ns();
            accumulator += now - previous_clock;
            previous_clock = now;

            while accumulator >= rate {
                accumulator -= rate;
            }

            thread::sleep(Duration::from_millis(((rate - accumulator) / 1000000) as u64));
        }
    });
}

fn main() {
    let mut file = File::open("Ball.c16").unwrap();
    let mut cartridge: Vec<u8> = Vec::new();
    file.read_to_end(&mut cartridge).unwrap();

    let header: &CH16Header = &cartridge[..16].into();
    let cart = &cartridge[16..];

    let buffer = Arc::new(Mutex::new(vec![0; WIDTH * HEIGHT]));

    let mut window =
        match minifb::Window::new("chip-16 emulator in Rust",
                                  WIDTH,
                                  HEIGHT,
                                  WindowOptions { scale: Scale::X2, ..WindowOptions::default() }) {
            Ok(win) => win,
            Err(err) => panic!("Unable to create window {}", err),
        };

    let chip16 = Arc::new(Mutex::new(CHIP16::new(header, cart)));

    let cpu_arc = buffer.clone();
    let c_a = chip16.clone();
    cpu_loop(1_000_000, move || c_a.lock().unwrap().cycle(&cpu_arc));

    let c_b = chip16.clone();
    draw_loop(60, || {
        if window.is_open() && !window.is_key_down(Key::Escape) {
            window.update_with_buffer(&buffer.lock().unwrap());

            c_b.lock().unwrap().vblank = true;

            State::Continue
        } else {
            State::Stop
        }
    });
}
