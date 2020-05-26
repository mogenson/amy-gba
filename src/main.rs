#![no_std]
#![feature(start)]
#![forbid(unsafe_code)]
#![feature(exclusive_range_pattern)]
#![feature(bindings_after_at)]

mod gba_display;
use gba_display::{GbaDisplay, PaletteColor};

use core::convert::{Infallible, TryFrom, TryInto};

use embedded_graphics::{
    fonts::{Font12x16, Text},
    image::Image,
    pixelcolor::Bgr555,
    prelude::*,
    primitives::{Circle, Line, Rectangle},
    style::{PrimitiveStyle, TextStyle},
};

use gba::{
    debug, fatal,
    io::{
        display::{DisplayControlSetting, DisplayMode, DisplayStatusSetting, DISPCNT, DISPSTAT},
        irq::{set_irq_handler, IrqEnableSetting, IrqFlags, BIOS_IF, IE, IF, IME},
        keypad::read_key_input,
    },
    oam::{write_obj_attributes, OBJAttr0, OBJAttr1, OBJAttr2, ObjectAttributes},
    palram::index_palram_obj_8bpp,
    vram::{bitmap::Mode3, get_8bpp_character_block, Tile8bpp},
    Color,
};

use tinytga::Tga;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    fatal!("{}", info);
    loop {}
}

#[start]
fn main(_argc: isize, _argv: *const *const u8) -> isize {
    debug!("Set up display");
    DISPCNT.write(
        DisplayControlSetting::new()
            .with_mode(DisplayMode::Mode3) // bitmap
            .with_bg2(true) // use background
            .with_obj(true) // use sprites
            .with_oam_memory_1d(true) // 1 dimensional vram mapping
            .with_force_vblank(true), // disable display
    );

    debug!("Register palette");
    register_palette();

    debug!("Draw reticle");
    draw_reticle().ok();

    debug!("Create display");
    let mut display = GbaDisplay;
    draw_tga(&mut display).ok();
    draw_text(&mut display).ok();

    debug!("Enable interrupts");
    set_irq_handler(irq_handler);
    DISPSTAT.write(DisplayStatusSetting::new().with_vblank_irq_enable(true));
    IE.write(IrqFlags::new().with_vblank(true));
    IME.write(IrqEnableSetting::IRQ_YES);

    const WIDTH: u32 = Mode3::WIDTH as u32;
    const HEIGHT: u32 = Mode3::HEIGHT as u32;
    let mut point = Point::try_from((WIDTH, HEIGHT)).unwrap() / 2;

    debug!("Start main loop");
    DISPCNT.write(DISPCNT.read().with_force_vblank(false)); // enable display

    loop {
        // sleep until vblank interrupt
        gba::bios::vblank_interrupt_wait();

        // read buttons input
        let input = read_key_input();

        // adjust game state and wait for vblank
        let offset = Point::new(input.x_tribool() as i32, input.y_tribool() as i32);
        point += offset;

        if let Ok((x @ 0..WIDTH, y @ 0..HEIGHT)) = point.try_into() {
            move_reticle(x as u16, y as u16);
            if input.a() {
                Pixel(Point::new(x as i32, y as i32), Bgr555::BLUE)
                    .draw(&mut display)
                    .ok();
            }
        } else {
            point -= offset; // undo
        }
    }
}

extern "C" fn irq_handler(flags: IrqFlags) {
    if flags.vblank() {
        BIOS_IF.write(BIOS_IF.read().with_vblank(true)); // clear vblank flag
        IF.write(IF.read().with_vblank(true));
    }
}

fn draw_tga(display: &mut GbaDisplay) -> Result<(), Infallible> {
    let tga = Tga::from_slice(include_bytes!("../assets/amy.tga")).unwrap();
    let image: Image<Tga, Bgr555> = Image::new(&tga, Point::zero());
    image.draw(display)?;
    Ok(())
}

fn draw_text(display: &mut GbaDisplay) -> Result<(), Infallible> {
    Text::new("Dirty Fucking Amy", Point::new(20, 20))
        .into_styled(TextStyle::new(Font12x16, Bgr555::CYAN))
        .draw(display)?;
    Rectangle::new(Point::new(15, 15), Point::new(227, 39))
        .into_styled(PrimitiveStyle::with_stroke(Bgr555::CYAN, 3))
        .draw(display)?;
    Ok(())
}

fn register_palette() {
    // slot 0 is for transparency
    index_palram_obj_8bpp(1).write(Color(Bgr555::BLACK.into_storage()));
    index_palram_obj_8bpp(2).write(Color(Bgr555::RED.into_storage()));
    index_palram_obj_8bpp(3).write(Color(Bgr555::GREEN.into_storage()));
    index_palram_obj_8bpp(4).write(Color(Bgr555::BLUE.into_storage()));
    index_palram_obj_8bpp(5).write(Color(Bgr555::YELLOW.into_storage()));
    index_palram_obj_8bpp(6).write(Color(Bgr555::MAGENTA.into_storage()));
    index_palram_obj_8bpp(7).write(Color(Bgr555::CYAN.into_storage()));
    index_palram_obj_8bpp(8).write(Color(Bgr555::WHITE.into_storage()));
}

fn draw_reticle() -> Result<(), Infallible> {
    let mut tile = Tile8bpp([PaletteColor::TANSPARENT.into_storage().into(); 16]);
    let style = PrimitiveStyle::with_stroke(PaletteColor::new(4), 1);

    Circle::new(Point::new(3, 3), 3)
        .into_styled(style)
        .draw(&mut tile)?;

    Line::new(Point::new(3, 0), Point::new(3, 6))
        .into_styled(style)
        .draw(&mut tile)?;

    Line::new(Point::new(0, 3), Point::new(6, 3))
        .into_styled(style)
        .draw(&mut tile)?;

    get_8bpp_character_block(5).index(1).write(tile);

    Ok(())
}

fn move_reticle(x: u16, y: u16) {
    write_obj_attributes(
        0,
        ObjectAttributes {
            attr0: OBJAttr0::new()
                .with_row_coordinate(y - 3)
                .with_is_8bpp(true),
            attr1: OBJAttr1::new().with_col_coordinate(x - 3),
            attr2: OBJAttr2::new().with_tile_id(514), // 8bpp tiles are even offset
        },
    );
}
