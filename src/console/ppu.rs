use crate::console::Memory;
use crate::console::*;
use super::regids::IF;

use sdl2::pixels::Color;
use sdl2::render::{Texture, Canvas};
use sdl2::surface::Surface;
use sdl2::video::Window;
use sdl2::rect::Point;

use std::collections::VecDeque;

const VB_0: u16 = 0x8000;
const VB_1: u16 = 0x8800;
const VB_2: u16 = 0x9000;

//tile map areas
const TMA_0: u16 = 0x9800;
const TMA_1: u16 = 0x9C00;

//registsers 
const LCDC: u16 = 0xFF40;
const LY: u16 = 0xFF44;
const LYC: u16 = 0xFF46;
const STAT: u16 = 0xFF41;
const SCY: u16 = 0xFF42;
const SCX: u16 = 0xFF43;
const WY: u16 = 0xFF4A;
const WX: u16 = 0xFF4B;
const DMA: u16 = 0xFF46;
const BGP: u16 = 0xFF47;
const OBP0: u16 = 0xFF48;
const OBP1: u16 = 0xFF49;
//const BGPS: u16 = 0xFF68;

// bit maks for each flag
const LCD_EN: u8 = 1 << 7;
const WIN_TM: u8 = 1 << 6;
const WIN_EN: u8 = 1 << 5;
const BGWIN_TILES: u8 = 1 << 4;
const BG_TM: u8 = 1 << 3;
const OBJ_S: u8 = 1 << 2;
const OBJ_EN: u8 = 1 << 1;
const BGWIN_EN: u8 = 1 << 0;

// bit masks for STAT reg
const LYC_INT_SEL: u8 = 1 << 6;
const M2_INT_SEL: u8 = 1 << 5;
const M1_INT_SEL: u8 = 1 << 4;
const M0_INT_SEL: u8 = 1 << 3;
const LYC_EQ_LY: u8 = 1 << 2;
const PPU_MODE: u8 = 0b11;


/*const PALETTE: [Color; 4] = [
    Color::RGB(0xe0, 0xf8, 0xd0), 
    Color::RGB(0x88, 0xc0, 0x70), 
    Color::RGB(0x34, 0x68, 0x56), 
    Color::RGB(0x08, 0x18, 0x20), 
];*/

const PALETTE: [[u8; 4]; 4] = [
    [0xE0, 0xF8, 0xD0, 0xFF],
    [0x88, 0xC0, 0x70, 0xFF],
    [0x34, 0x68, 0x56, 0xFF],
    [0x08, 0x18, 0x20, 0xFF],
];

const PALETTE_OBJ: [[u8; 4]; 4] = [
    [0xE0, 0xF8, 0xD0, 0x00],
    [0x88, 0xC0, 0x70, 0xFF],
    [0x34, 0x68, 0x56, 0xFF],
    [0x08, 0x18, 0x20, 0xFF],
];

//https://gbdev.io/pandocs/pixel_fifo.html
#[derive(Debug)]
struct Pixel {
    pub color: u8,
    pub palette: u8,
    pub sprite_prio: u8,
    pub bg_prio: u8,
}

impl Pixel {
    pub fn new(color: u8, palette: u8, sprite_prio: u8, bg_prio: u8) -> Pixel {
        Pixel {
            color,
            palette,
            sprite_prio,
            bg_prio
        }
    }
}

pub struct PPU {
    dots: usize,
    mode: u8,
    
    //fetcher
    fx: u8,
    fy: u8,

    // registers
    lcdc: u8,
    ly: u8,
    lyc: u8,
    stat: u8,
    scy: u8,
    scx: u8,
    wy: u8,
    wx: u8,
    bgp: u8,
    obp0: u8,
    obp1: u8,

    bg_fifo: VecDeque<Pixel>,
    obj_fifo: VecDeque<Pixel>,
    bg: [u8; LCD_SIZE * 4],

}

impl PPU {
    pub fn new() -> PPU {
        PPU {
            dots: 0,
            mode: 0,
            
            fx: 0,
            fy: 0,

            lcdc: 0u8,
            ly: 0u8,
            lyc: 0u8,
            stat: 0u8,
            scy: 0u8,
            scx: 0u8,
            wy: 0u8,
            wx: 0u8,
            bgp: 0u8,
            obp0: 0u8,
            obp1: 0u8,

            bg_fifo: VecDeque::new(),
            obj_fifo: VecDeque::new(),
            bg: [0x00; LCD_SIZE * 4],
        }
    }

    fn update_registers(&mut self, memory: &Memory) {
        self.lcdc = memory.read(LCDC); 
        self.lyc = memory.read(LYC); 
        self.stat = memory.read(STAT); 
        self.scy = memory.read(SCY); 
        self.scx = memory.read(SCX); 
        self.wy = memory.read(WY); 
        self.wx = memory.read(WX); 
        self.bgp = memory.read(BGP); 
        self.obp0 = memory.read(OBP0); 
        self.obp1 = memory.read(OBP1); 
    }

    fn set_registers(&mut self, memory: &mut Memory) {
        memory.write(LY, self.ly);
    }

    pub fn request_vblank_interrupt(&mut self, memory: &mut Memory){
        let if_old = memory.read(IF);
        let if_new = if_old | 0b1 ;
        memory.write(IF, if_new);
    }

    pub fn request_stat_interrupt(&mut self, memory: &mut Memory){
        let if_old = memory.read(IF);
        let if_new = if_old | 0b10;
        memory.write(IF, if_new);
    }

    fn get_tile(&mut self, memory: &mut Memory) -> u16 {

        let in_window = if self.fx >= (self.wx.overflowing_sub(8).0) / 8 && self.fy >= self.wy {
            true
        } else { false };

        let win_tma = if self.check_lcdc(WIN_TM) {
            TMA_0
        } else { TMA_1 };

        let bg_tma = if !self.check_lcdc(BG_TM) {
            TMA_0
        } else { TMA_1 };

        let tma = if self.check_lcdc(WIN_TM) && in_window {
            win_tma
        } else { bg_tma };
        self.fx = (self.fx + self.scx / 8) & 0x1F;
        self.fy = (self.ly.overflowing_add(self.scy).0) & 0xFF;

        if in_window {
            self.fx = (self.wx.overflowing_sub(7).0 / 8) & 0x1F;
            self.fy = self.wy;
        }

        let block_y = self.fy as u16 / 8;
        let loc = tma + self.fx as u16 + 32 * block_y ;
        //println!("{loc:#04X}, fetcher: {}, {}", self.fx, block_y);

        memory.read(loc) as u16
    }
    
    fn get_obj(&mut self, memory: &mut Memory) -> Vec<u16> {
        
        let mut valid_objects = Vec::new();

        // 0 for 1 tile, 1 for 2 tiles
        let range = if !self.check_lcdc(OBJ_S) {
            8
        }else {
            16
        };

        let mut obj_counter = 0;
        for addr in (0xFE00..0xFE9F).step_by(4) {
            let y = memory.read(addr);
            //let x = memory.read(addr + 1);
            //let tile_index = memory.read(addr + 2) as u16;
            //let attributes = memory.read(addr + 3);

            //means the object is on the current scanline
            if y as u32 > self.fy as u32 + 8 && y as u32 <= self.fy as u32 + 8 + range as u32 {
                
                valid_objects.push(addr);
                obj_counter += 1;

                if obj_counter >= 10 {
                    break
                }
            }


        }

        valid_objects
    }

    fn mix_bytes(low: u8, high: u8) -> [u8; 8] {
        let mut pixels = [0u8; 8];

        for i in 0..8 {
            let bit = 0x1 << i as u8;
            let lsb = (low & bit) >> i;
            let msb = (high & bit) >> i;
            pixels[7 - i] = lsb + (msb << 1);
        }

        pixels
    }

    pub fn update(&mut self, memory: &mut Memory){
        self.update_registers(memory);

        if !self.check_lcdc(BGWIN_EN) {
            //println!("here")
        }

        if !self.check_lcdc(LCD_EN) {
            //self.clear();
            //return
        }

        if self.ly == self.ly && self.stat & 0b01000000 != 0 {
            self.request_stat_interrupt(memory);
            self.stat |= 0b10;
        }

        self.do_scan_line(memory);

        self.set_registers(memory);
    }


    /*
     *
     * Modify scan line to go pixel by pixel, this means that mixing bytes will have to be changed
     * A way to index 2 bits from bytes will be needed
     * for x in scanlineLength:
     *  fx = x + scx / 8
     *  fy = y + scy
     *
     *  etc
     */
    fn do_scan_line(&mut self, memory: &mut Memory) {
        self.fx = 0;

        if self.ly <= 143 {

            // get valid objects to be drawn
            self.mode = 2;
            let objects = self.get_obj(memory);
            if self.stat & 0b00100000 != 0 {
                self.request_stat_interrupt(memory);
            }

            self.mode = 3;

            //render each tile
            for i in 0..20 {
                let tile_index = self.get_tile(memory);

                let vram_bank = if self.check_lcdc(0b0001_0000) {
                    if tile_index < 128 {
                        VB_0
                    } else {
                        VB_1
                    }
                } else {
                    if tile_index < 128 {
                        VB_2
                    } else {
                        VB_0
                    }
                };
                /*if self.lcdc & WIN_EN != 0 {
                    vram_bank = if self.lcdc & BGWIN_TILES != 0{
                        VB_0
                    } else { VB_1 };
                }*/
                let index_low = vram_bank + tile_index * 16 + (self.fy as u16 % 8) * 2;
                let index_high = index_low + 1;
                let low = memory.read(index_low);
                let high = memory.read(index_high);

                let pixels = PPU::mix_bytes(low, high);
                for p in 0..8 {
                    self.bg_fifo.push_back(Pixel::new(pixels[p], 0, 0, 0));
                }
                
                for addr in &objects {
                    //let y = memory.read(addr + 0);
                    let x = memory.read(*addr + 1) - 8;
                    if x / 8 == i {
                        let obj_ti = memory.read(*addr + 2) as u16;
                        let attributes = memory.read(addr + 3);
                        let index_low = VB_0 + obj_ti * 16 + (self.fy as u16 % 8) * 2;
                        let index_high = index_low + 1;
                        let palette = if attributes & 0b00010000 != 0 {
                            memory.read(OBP0)
                        } else { memory.read(OBP1) };
                        let low = memory.read(index_low);
                        let high = memory.read(index_high);

                        let obj_pixels = PPU::mix_bytes(low, high);
                        for p in 0..8 {
                            self.obj_fifo.push_back(Pixel::new(obj_pixels[p], palette, 0, attributes & 0x80 >> 7));
                        }
                    }
                }


                self.internal_render(i as usize);

                self.fx = self.fx + 1;
            }
            self.mode = 0;

        }
        if self.ly > 143 {
            self.request_vblank_interrupt(memory);
            self.mode = 1;
        }

        self.ly = (self.ly + 1) % 154;
        self.stat |= self.mode;
    }

    fn internal_render(&mut self, x: usize) {
        for i in 0..8 {
            let pixel = self.bg_fifo.pop_front().unwrap_or(Pixel::new(0, 0, 0, 0));
            let obj_pixel = self.obj_fifo.pop_front().unwrap_or(Pixel::new(0, 0, 0, 1));
            
            let index = (x * 8 + i + LCD_WIDTH * self.ly as usize) * 4;

            if obj_pixel.bg_prio == 1 {
                self.bg[index] = PALETTE[pixel.color as usize][3];
                self.bg[index + 1] = PALETTE[pixel.color as usize][2];
                self.bg[index + 2] = PALETTE[pixel.color as usize][1];
                self.bg[index + 3] = PALETTE[pixel.color as usize][0];
            } else {
                self.bg[index] = PALETTE_OBJ[obj_pixel.color as usize][3];
                self.bg[index + 1] = PALETTE_OBJ[obj_pixel.color as usize][2];
                self.bg[index + 2] = PALETTE_OBJ[obj_pixel.color as usize][1];
                self.bg[index + 3] = PALETTE_OBJ[obj_pixel.color as usize][0];
            }

        }
    }

    fn clear(&mut self) {
        for _i in 0..LCD_SIZE * 4 - 4{
            self.bg[_i] = PALETTE[0][3];
            self.bg[_i + 1] = PALETTE[0][2];
            self.bg[_i + 2] = PALETTE[0][1];
            self.bg[_i + 3] = PALETTE[0][0];
        }
    }

    pub fn is_ready(&self) -> bool {
        if self.ly == 153 {
            return true
        }
        false
    }

    fn check_lcdc(&self, mask: u8) -> bool {
        self.lcdc & mask != 0
    }

    pub fn render(&mut self, texture: &mut Texture) -> Result<(), String> {

        texture.update(None, &mut self.bg, LCD_WIDTH * 4).unwrap();
        Ok(())

    }


}
