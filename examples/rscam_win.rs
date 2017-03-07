#![feature(plugin)]



extern crate libc;
extern crate quirc;
extern crate rscam;
extern crate rustc_serialize;
extern crate sdl;
extern crate time;
extern crate mjpeg_sys;
extern crate quirc_sys;

use libc::{ c_char, c_int, c_void, uint8_t, uint32_t };
use mjpeg_sys::MjpegDecoder;
use quirc::parse_size;
use quirc_sys::{ convert, Quirc };
use rscam::Camera;
use sdl::event::{Event, Key};
use sdl::video::{ll, Surface, SurfaceFlag, VideoFlag};
use std::ffi::CString;
use std::io;

struct RunParms {
    device: String,
    width: u32,
    height: u32,
    verbose: bool,
    show_frame_rate: bool,
}

// I'm not quite sure why the rust-sdk doesn't expose these.
#[link(name="SDL_gfx")]
extern { // from SDL_gfxPrimitives.h

    pub fn stringColor(surface: *mut ll::SDL_Surface, x: i16, y: i16, s: *const c_char, color: uint32_t) -> c_int;
    pub fn lineColor(surface: *mut ll::SDL_Surface, x1: i16, y1: i16, x2: i16, y2: i16, color: uint32_t) -> c_int;
}


fn fat_text(surface: &Surface, x: c_int, y: c_int, text: &str) {

    let cstr = CString::new(text).unwrap();

    for i in -1..2 {
        for j in -1..2 {
            unsafe { stringColor(surface.raw, (x + i) as i16, (y + j) as i16, cstr.as_ptr(), 0x000000ff) };
        }
    }
    unsafe { stringColor(surface.raw, x as i16, y as i16, cstr.as_ptr(), 0xff0000ff) };
}

fn fat_text_centered(surface: &Surface, x: c_int, y: c_int, text: &str) {
	fat_text(surface, x - (text.len() * 4) as c_int, y, text);
}


fn draw_qr(parms: &RunParms, surface: &Surface, qr:&mut Quirc) {
    let num_codes = qr.count();
    //println!("Decoded {} codes", num_codes);
    for code_idx in 0..num_codes {
        let code = qr.extract(code_idx);

        let mut xc = 0;
        let mut yc = 0;

        for j in 0..4 {
            let p1 = &code.corners[j];
            let p2 = &code.corners[(j + 1) % 4];

            xc += p1.x;
            yc += p1.y;

            unsafe { lineColor(surface.raw, p1.x as i16, p1.y as i16, p2.x as i16, p2.y as i16, 0xff0000ff) };
        }
        xc /= 4;
        yc /= 4;

        if parms.verbose {
            fat_text_centered(surface, xc, yc - 20, &format!("Code size: {} cells", code.size));
        }

        match code.decode() {
            Ok(data) => {
                let qr_text = &format!("{}", data);
                println!("==> {} (len {})", qr_text, qr_text.len());
                fat_text_centered(surface, xc, yc, qr_text);
            },
            Err(err) => {
                println!("Error: {}", err);
            }
        };
    }
}

fn run_loop(parms: &RunParms, cam: &mut Camera, screen: &Surface, qr: &mut Quirc, decoder: &mut MjpegDecoder) -> io::Result<()> {

    let mut frame_count = 0;
    let mut last_rate_sec = time::now().tm_sec;
    let mut frame_rate_text = "".to_owned();

    println!("Press ESC or close the window to exit");
    loop {
        let frame = try!(cam.capture());

        let frame_len = frame.len();
        let frame_data = &frame[..] as *const _ as *const c_void;

        screen.lock();
        let surface: ll::SDL_Surface = unsafe { (*screen.raw) };
        try!(decoder.decode_rgb32(frame_data as *const c_void, frame_len,
                                  surface.pixels as *mut c_void,
                                  surface.pitch as i32,
                                  surface.w, surface.h ));

        unsafe { convert::rgb32_to_luma(surface.pixels as *mut uint8_t,
                                        surface.pitch as i32,
                                        surface.w, surface.h,
                                        qr.begin(),
                                        surface.w) };
        qr.end();

        screen.unlock();

        draw_qr(parms, screen, qr);

        if parms.show_frame_rate {
            fat_text(screen, 5, 5, &frame_rate_text);
        }

        screen.flip();

        loop {
            match sdl::event::poll_event() {
                Event::Quit => return Ok(()),
                Event::None => break,
                Event::Key(k, _, _, _)
                    if k == Key::Escape
                        => return Ok(()),
                _ => {}
            }
        }
        if parms.show_frame_rate {
            let now = time::now();
            if now.tm_sec != last_rate_sec {
                frame_rate_text = format!("Frame rate: {} fps", frame_count);
                frame_count = 0;
                last_rate_sec = now.tm_sec;
            }
        }
        frame_count += 1;
    }
}


fn run(parms: &RunParms) -> io::Result<()> {

    let mut cam = try!(rscam::new(&parms.device));

    cam.start(&rscam::Config {
        interval: (1, 30),
        resolution: (parms.width, parms.height),
        format: b"MJPG",
        ..Default::default()
    }).unwrap();

    let mut qr = Quirc::new();

    // Note: it's important that the parameters to qr.resize match the
    //       screen size since we use the screen buffer as the source for
    // the qr decoder.
    try!(qr.resize(parms.width as isize, parms.height as isize));

    sdl::init(&[sdl::InitFlag::Video]);
    let screen = match sdl::video::set_video_mode(parms.width as isize, parms.height as isize, 32,
                                                  &[SurfaceFlag::HWSurface],
                                                  &[VideoFlag::DoubleBuf]) {
        Ok(screen) => screen,
        Err(err) => panic!("failed to set video mode: {}", err)
    };

    let mut decoder = MjpegDecoder::new();
    try!(run_loop(parms, &mut cam, &screen, &mut qr, &mut decoder));

    sdl::quit();
    Ok(())
}


fn main() {

    let mut run_parms = RunParms {
        device: "/dev/video0".to_owned(),
        width: 640,
        height: 480,
        verbose: false,
        show_frame_rate: true,
    };

    if let Err(err) = run(&run_parms) {
        println!("Error: {}", err);
    }
}
