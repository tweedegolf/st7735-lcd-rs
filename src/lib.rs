#![no_std]

//! This crate provides a ST7735 driver to connect to TFT displays.

pub mod instruction;

use crate::instruction::Instruction;
use num_derive::ToPrimitive;
use num_traits::ToPrimitive;

use embedded_hal::blocking::delay::DelayMs;
use embedded_hal::digital::v2::OutputPin;

use async_spi::SPI;

/// ST7735 driver to connect to TFT displays.
pub struct ST7735<SPIH, DC, RST>
where
    SPIH: async_spi::SPIHardware + 'static,
    DC: OutputPin,
    RST: OutputPin,
{
    /// SPI
    spi: SPI<SPIH>,

    /// Data/command pin.
    dc: DC,

    /// Reset pin.
    rst: RST,

    /// Whether the display is RGB (true) or BGR (false)
    rgb: bool,

    /// Whether the colours are inverted (true) or not (false)
    inverted: bool,

    /// Global image offset
    dx: u16,
    dy: u16,
    width: u32,
    height: u32,
}

/// Display orientation.
#[derive(ToPrimitive)]
pub enum Orientation {
    Portrait = 0x00,
    Landscape = 0x60,
    PortraitSwapped = 0xC0,
    LandscapeSwapped = 0xA0,
}

impl<SPIH, DC, RST> ST7735<SPIH, DC, RST>
where
    SPIH: async_spi::SPIHardware + 'static,
    DC: OutputPin,
    RST: OutputPin,
{
    /// Creates a new driver instance that uses hardware SPI.
    pub fn new(
        spi: SPI<SPIH>,
        dc: DC,
        rst: RST,
        rgb: bool,
        inverted: bool,
        width: u32,
        height: u32,
    ) -> Self {
        let display = ST7735 {
            spi,
            dc,
            rst,
            rgb,
            inverted,
            dx: 0,
            dy: 0,
            width,
            height,
        };

        display
    }

    /// Runs commands to initialize the display.
    pub async fn init<DELAY>(&mut self, delay: &mut DELAY) -> Result<(), ()>
    where
        DELAY: DelayMs<u8>,
    {
        self.hard_reset(delay)?;
        self.write_command(Instruction::SWRESET, None).await?;
        delay.delay_ms(200);
        self.write_command(Instruction::SLPOUT, None).await?;
        delay.delay_ms(200);
        self.write_command(Instruction::FRMCTR1, Some(&[0x01, 0x2C, 0x2D]))
            .await?;
        self.write_command(Instruction::FRMCTR2, Some(&[0x01, 0x2C, 0x2D]))
            .await?;
        self.write_command(
            Instruction::FRMCTR3,
            Some(&[0x01, 0x2C, 0x2D, 0x01, 0x2C, 0x2D]),
        )
        .await?;
        self.write_command(Instruction::INVCTR, Some(&[0x07]))
            .await?;
        self.write_command(Instruction::PWCTR1, Some(&[0xA2, 0x02, 0x84]))
            .await?;
        self.write_command(Instruction::PWCTR2, Some(&[0xC5]))
            .await?;
        self.write_command(Instruction::PWCTR3, Some(&[0x0A, 0x00]))
            .await?;
        self.write_command(Instruction::PWCTR4, Some(&[0x8A, 0x2A]))
            .await?;
        self.write_command(Instruction::PWCTR5, Some(&[0x8A, 0xEE]))
            .await?;
        self.write_command(Instruction::VMCTR1, Some(&[0x0E]))
            .await?;
        if self.inverted {
            self.write_command(Instruction::INVON, None).await?;
        } else {
            self.write_command(Instruction::INVOFF, None).await?;
        }
        if self.rgb {
            self.write_command(Instruction::MADCTL, Some(&[0x00]))
                .await?;
        } else {
            self.write_command(Instruction::MADCTL, Some(&[0x08]))
                .await?;
        }
        self.write_command(Instruction::COLMOD, Some(&[0x05]))
            .await?;
        self.write_command(Instruction::DISPON, None).await?;
        delay.delay_ms(200);
        Ok(())
    }

    pub fn hard_reset<DELAY>(&mut self, delay: &mut DELAY) -> Result<(), ()>
    where
        DELAY: DelayMs<u8>,
    {
        self.rst.set_high().map_err(|_| ())?;
        delay.delay_ms(10);
        self.rst.set_low().map_err(|_| ())?;
        delay.delay_ms(10);
        self.rst.set_high().map_err(|_| ())
    }

    async fn write_command(
        &mut self,
        command: Instruction,
        params: Option<&[u8]>,
    ) -> Result<(), ()> {
        self.dc.set_low().map_err(|_| ())?;
        self.spi
            .write(&[command.to_u8().unwrap()])
            .await
            .map_err(|_| ())?;
        if params.is_some() {
            self.start_data()?;
            self.write_data(params.unwrap()).await?;
        }
        Ok(())
    }

    fn start_data(&mut self) -> Result<(), ()> {
        self.dc.set_high().map_err(|_| ())
    }

    async fn write_data(&mut self, data: &[u8]) -> Result<(), ()> {
        self.spi.write(data).await.map_err(|_| ())
    }

    /// Writes a data word to the display.
    async fn write_word(&mut self, value: u16) -> Result<(), ()> {
        self.write_data(&value.to_be_bytes()).await
    }

    async fn write_words_buffered(
        &mut self,
        words: impl IntoIterator<Item = u16>,
    ) -> Result<(), ()> {
        let mut buffer = [0; 32];
        let mut index = 0;
        for word in words {
            let as_bytes = word.to_be_bytes();
            buffer[index] = as_bytes[0];
            buffer[index + 1] = as_bytes[1];
            index += 2;
            if index >= buffer.len() {
                self.write_data(&buffer).await?;
                index = 0;
            }
        }
        self.write_data(&buffer[0..index]).await
    }

    pub async fn set_orientation(&mut self, orientation: &Orientation) -> Result<(), ()> {
        if self.rgb {
            self.write_command(Instruction::MADCTL, Some(&[orientation.to_u8().unwrap()]))
                .await?;
        } else {
            self.write_command(
                Instruction::MADCTL,
                Some(&[orientation.to_u8().unwrap() | 0x08]),
            )
            .await?;
        }
        Ok(())
    }

    /// Sets the global offset of the displayed image
    pub fn set_offset(&mut self, dx: u16, dy: u16) {
        self.dx = dx;
        self.dy = dy;
    }

    /// Sets the address window for the display.
    async fn set_address_window(&mut self, sx: u16, sy: u16, ex: u16, ey: u16) -> Result<(), ()> {
        self.write_command(Instruction::CASET, None).await?;
        self.start_data()?;
        self.write_word(sx + self.dx).await?;
        self.write_word(ex + self.dx).await?;
        self.write_command(Instruction::RASET, None).await?;
        self.start_data()?;
        self.write_word(sy + self.dy).await?;
        self.write_word(ey + self.dy).await
    }

    /// Sets a pixel color at the given coords.
    pub async fn set_pixel(&mut self, x: u16, y: u16, color: u16) -> Result<(), ()> {
        self.set_address_window(x, y, x, y).await?;
        self.write_command(Instruction::RAMWR, None).await?;
        self.start_data()?;
        self.write_word(color).await
    }

    /// Writes pixel colors sequentially into the current drawing window
    pub async fn write_pixels<P: IntoIterator<Item = u16>>(&mut self, colors: P) -> Result<(), ()> {
        self.write_command(Instruction::RAMWR, None).await?;
        self.start_data()?;
        for color in colors {
            self.write_word(color).await?;
        }
        Ok(())
    }
    pub async fn write_pixels_buffered<P: IntoIterator<Item = u16>>(
        &mut self,
        colors: P,
    ) -> Result<(), ()> {
        self.write_command(Instruction::RAMWR, None).await?;
        self.start_data()?;
        self.write_words_buffered(colors).await
    }

    /// Sets pixel colors at the given drawing window
    pub async fn set_pixels<P: IntoIterator<Item = u16>>(
        &mut self,
        sx: u16,
        sy: u16,
        ex: u16,
        ey: u16,
        colors: P,
    ) -> Result<(), ()> {
        self.set_address_window(sx, sy, ex, ey).await?;
        self.write_pixels(colors).await
    }

    pub async fn set_pixels_buffered<P: IntoIterator<Item = u16>>(
        &mut self,
        sx: u16,
        sy: u16,
        ex: u16,
        ey: u16,
        colors: P,
    ) -> Result<(), ()> {
        self.set_address_window(sx, sy, ex, ey).await?;
        self.write_pixels_buffered(colors).await
    }
}

#[cfg(feature = "graphics")]
extern crate embedded_graphics;
#[cfg(feature = "graphics")]
use self::embedded_graphics::{
    drawable::Pixel,
    image::Image,
    pixelcolor::{
        raw::{RawData, RawU16},
        Rgb565,
    },
    prelude::*,
    primitives::Rectangle,
    style::{PrimitiveStyle, Styled},
    DrawTarget,
};

#[cfg(feature = "graphics")]
// async note: upstream trait is not async.
// impl<SPIH, DC, RST> DrawTarget<Rgb565> for ST7735<SPIH, DC, RST>
impl<SPIH, DC, RST> ST7735<SPIH, DC, RST>
where
    SPIH: async_spi::SPIHardware + 'static,
    DC: OutputPin,
    RST: OutputPin,
{
    pub async fn draw_pixel(&mut self, pixel: Pixel<Rgb565>) -> Result<(), ()> {
        let Pixel(Point { x, y }, color) = pixel;
        self.set_pixel(x as u16, y as u16, RawU16::from(color).into_inner())
            .await
    }

    /// Draws an object from an iterator over its pixels.
    pub async fn draw_iter<T>(&mut self, item: T) -> Result<(), ()>
    where
        T: IntoIterator<Item = Pixel<Rgb565>>,
    {
        for pixel in item {
            self.draw_pixel(pixel).await?;
        }

        Ok(())
    }

    pub async fn draw_rectangle(
        &mut self,
        item: &Styled<Rectangle, PrimitiveStyle<Rgb565>>,
    ) -> Result<(), ()> {
        let shape = item.primitive;
        let rect_width = shape.bottom_right.x - item.primitive.top_left.x + 1;
        let rect_height = shape.bottom_right.y - item.primitive.top_left.y + 1;
        let rect_size = rect_width * rect_height;

        match (item.style.fill_color, item.style.stroke_color) {
            (Some(fill), None) => {
                let color = RawU16::from(fill).into_inner();
                let iter = (0..rect_size).map(move |_| color);
                self.set_pixels_buffered(
                    shape.top_left.x as u16,
                    shape.top_left.y as u16,
                    shape.bottom_right.x as u16,
                    shape.bottom_right.y as u16,
                    iter,
                )
                .await
            }
            (Some(fill), Some(stroke)) => {
                let fill_color = RawU16::from(fill).into_inner();
                let stroke_color = RawU16::from(stroke).into_inner();
                let iter = (0..rect_size).map(move |i| {
                    if i % rect_width <= item.style.stroke_width as i32
                        || i % rect_width >= rect_width - item.style.stroke_width as i32
                        || i <= item.style.stroke_width as i32 * rect_width
                        || i >= (rect_height - item.style.stroke_width as i32) * rect_width
                    {
                        stroke_color
                    } else {
                        fill_color
                    }
                });
                self.set_pixels_buffered(
                    shape.top_left.x as u16,
                    shape.top_left.y as u16,
                    shape.bottom_right.x as u16,
                    shape.bottom_right.y as u16,
                    iter,
                )
                .await
            }
            // TODO: Draw edges as subrectangles
            (None, Some(_)) => self.draw_iter(item).await,
            (None, None) => self.draw_iter(item).await,
        }
    }

    pub async fn draw_image<'a, 'b, I>(&mut self, item: &'a Image<'b, I, Rgb565>) -> Result<(), ()>
    where
        &'b I: IntoPixelIter<Rgb565>,
        I: ImageDimensions,
    {
        let sx = item.top_left().x as u16;
        let sy = item.top_left().y as u16;
        let ex = item.bottom_right().x as u16;
        let ey = item.bottom_right().y as u16;
        // -1 is required because image gets skewed if it is not present
        // NOTE: Is this also required for draw_rect?
        self.set_pixels_buffered(
            sx,
            sy,
            ex - 1,
            ey - 1,
            item.into_iter().map(|p| RawU16::from(p.1).into_inner()),
        )
        .await
    }

    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }
}
