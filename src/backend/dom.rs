use std::{cell::RefCell, io::Result as IoResult, rc::Rc};

use ratatui::{
    backend::WindowSize,
    buffer::Cell,
    layout::{Position, Size},
    prelude::Backend,
};
use web_sys::{
    wasm_bindgen::{prelude::Closure, JsCast},
    window, Document, Element, Window,
};

use crate::{backend::utils::*, error::Error, widgets::hyperlink::HYPERLINK_MODIFIER};

/// DOM backend.
///
/// This backend uses the DOM to render the content to the screen.
///
/// In other words, it transforms the [`Cell`]s into `<span>`s which are then
/// appended to a `<pre>` element.
#[derive(Debug)]
pub struct DomBackend {
    /// Whether the backend has been initialized.
    initialized: Rc<RefCell<bool>>,
    /// Current buffer.
    buffer: Vec<Vec<Cell>>,
    /// Previous buffer.
    prev_buffer: Vec<Vec<Cell>>,
    /// Cells.
    cells: Vec<Element>,
    /// Grid element.
    grid: Element,
    /// Window.
    window: Window,
    /// Document.
    document: Document,
}

impl DomBackend {
    /// Constructs a new [`DomBackend`].
    pub fn new() -> Result<Self, Error> {
        let window = window().ok_or(Error::UnableToRetrieveWindow)?;
        let document = window.document().ok_or(Error::UnableToRetrieveDocument)?;
        let mut backend = Self {
            initialized: Rc::new(RefCell::new(false)),
            buffer: vec![],
            prev_buffer: vec![],
            cells: vec![],
            grid: document.create_element("div")?,
            window,
            document,
        };
        backend.add_on_resize_listener();
        backend.reset_grid()?;
        Ok(backend)
    }

    /// Add a listener to the window resize event.
    fn add_on_resize_listener(&mut self) {
        let initialized = self.initialized.clone();
        let closure = Closure::<dyn FnMut(_)>::new(move |_: web_sys::Event| {
            initialized.replace(false);
        });
        self.window
            .set_onresize(Some(closure.as_ref().unchecked_ref()));
        closure.forget();
    }

    /// Reset the grid and clear the cells.
    fn reset_grid(&mut self) -> Result<(), Error> {
        self.grid = self.document.create_element("div")?;
        self.grid.set_attribute("id", "grid")?;
        self.cells.clear();
        self.buffer = get_sized_buffer();
        self.prev_buffer = self.buffer.clone();
        Ok(())
    }

    /// Pre-render the content to the screen.
    ///
    /// This function is called from [`flush`] once to render the initial
    /// content to the screen.
    fn prerender(&mut self) -> Result<(), Error> {
        for line in self.buffer.iter() {
            let mut line_cells: Vec<Element> = Vec::new();
            let mut hyperlink: Vec<Cell> = Vec::new();
            for (i, cell) in line.iter().enumerate() {
                if cell.modifier.contains(HYPERLINK_MODIFIER) {
                    hyperlink.push(cell.clone());
                    // If the next cell is not part of the hyperlink, close it
                    if !line
                        .get(i + 1)
                        .map(|c| c.modifier.contains(HYPERLINK_MODIFIER))
                        .unwrap_or(false)
                    {
                        let anchor = create_anchor(&self.document, &hyperlink)?;
                        for link_cell in &hyperlink {
                            let span = create_span(&self.document, link_cell)?;
                            self.cells.push(span.clone());
                            anchor.append_child(&span)?;
                        }
                        line_cells.push(anchor);
                        hyperlink.clear();
                    }
                } else {
                    let span = create_span(&self.document, cell)?;
                    self.cells.push(span.clone());
                    line_cells.push(span);
                }
            }

            // Create a <pre> element for the line
            let pre = self.document.create_element("pre")?;

            // Append all elements (spans and anchors) to the <pre>
            for elem in line_cells {
                pre.append_child(&elem)?;
            }

            // Append the <pre> to the grid
            self.grid.append_child(&pre)?;
        }
        Ok(())
    }

    /// Compare the current buffer to the previous buffer and updates the grid
    /// accordingly.
    fn update_grid(&mut self) -> Result<(), Error> {
        for (y, line) in self.buffer.iter().enumerate() {
            for (x, cell) in line.iter().enumerate() {
                if cell.modifier.contains(HYPERLINK_MODIFIER) {
                    continue;
                }
                if cell != &self.prev_buffer[y][x] {
                    let elem = self.cells[y * self.buffer[0].len() + x].clone();
                    elem.set_inner_html(cell.symbol());
                    elem.set_attribute("style", &get_cell_style_as_css(cell))?;
                }
            }
        }
        Ok(())
    }
}

impl Backend for DomBackend {
    // Populates the buffer with the given content.
    fn draw<'a, I>(&mut self, content: I) -> IoResult<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        if !*self.initialized.borrow() {
            // Only runs on resize event.
            if let Some(grid) = self.document.get_element_by_id("grid") {
                grid.remove();
                self.reset_grid()?;
            }
        }

        // Update the cells with new content
        for (x, y, cell) in content {
            let y = y as usize;
            let x = x as usize;
            if y < self.buffer.len() {
                let line = &mut self.buffer[y];
                line.extend(
                    std::iter::repeat_with(Cell::default).take(x.saturating_sub(line.len())),
                );
                if x < line.len() {
                    line[x] = cell.clone();
                }
            }
        }
        Ok(())
    }

    /// Flush the content to the screen.
    ///
    /// This function is called after the [`DomBackend::draw`] function to
    /// actually render the content to the screen.
    fn flush(&mut self) -> IoResult<()> {
        if !*self.initialized.borrow() {
            self.initialized.replace(true);
            let body = self.document.body().ok_or(Error::UnableToRetrieveBody)?;
            body.append_child(&self.grid).map_err(Error::from)?;
            self.prerender()?;
            // Set the previous buffer to the current buffer for the first render
            self.prev_buffer = self.buffer.clone();
        }
        // Check if the buffer has changed since the last render and update the grid
        if self.buffer != self.prev_buffer {
            self.update_grid()?;
        }
        self.prev_buffer = self.buffer.clone();
        Ok(())
    }

    fn hide_cursor(&mut self) -> IoResult<()> {
        Ok(())
    }

    fn show_cursor(&mut self) -> IoResult<()> {
        Ok(())
    }

    fn get_cursor(&mut self) -> IoResult<(u16, u16)> {
        Ok((0, 0))
    }

    fn set_cursor(&mut self, _x: u16, _y: u16) -> IoResult<()> {
        Ok(())
    }

    fn clear(&mut self) -> IoResult<()> {
        self.buffer = get_sized_buffer();
        Ok(())
    }

    fn size(&self) -> IoResult<Size> {
        Ok(Size::new(
            self.buffer[0].len().saturating_sub(1) as u16,
            self.buffer.len().saturating_sub(1) as u16,
        ))
    }

    fn window_size(&mut self) -> IoResult<WindowSize> {
        unimplemented!()
    }

    fn get_cursor_position(&mut self) -> IoResult<Position> {
        unimplemented!()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, _: P) -> IoResult<()> {
        unimplemented!()
    }
}
