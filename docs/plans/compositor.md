# Compositor Design (Future Work)

> ⚠️ **Status**: This is a design document for a planned feature. The compositor has not been implemented - the system currently uses xterm.js for terminal rendering.

The compositor manages windows and renders the GUI using Canvas2D.

## Architecture

```
┌──────────────────────────────────────────┐
│              Compositor                  │
│  ┌─────────────┐  ┌─────────────────┐   │
│  │   Layout    │  │     Surface     │   │
│  │ (Tiling BSP)│  │   (Canvas2D)    │   │
│  └──────┬──────┘  └────────┬────────┘   │
│         │                  │            │
│  ┌──────▼──────────────────▼──────┐     │
│  │          Windows               │     │
│  │  ┌────────┐  ┌────────┐       │     │
│  │  │Terminal│  │ Files  │  ...  │     │
│  │  └────────┘  └────────┘       │     │
│  └────────────────────────────────┘     │
└──────────────────────────────────────────┘
```

## Components

### Compositor

Main coordinator:

```rust
pub struct Compositor {
    windows: Vec<Window>,
    layout: TilingLayout,
    surface: Option<Surface>,
    focused: Option<usize>,
}
```

### Window

Individual window:

```rust
pub struct Window {
    pub id: WindowId,
    pub title: String,
    pub owner: TaskId,
    pub rect: Rect,
    pub dirty: bool,
}
```

### Layout

Binary Space Partition for tiling:

```rust
pub struct TilingLayout {
    root: Option<LayoutNode>,
    bounds: Rect,
}

enum LayoutNode {
    Window(WindowId),
    Split {
        direction: SplitDirection,
        ratio: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}
```

### Surface

Canvas2D rendering:

```rust
pub struct Surface {
    canvas: HtmlCanvasElement,
    context: CanvasRenderingContext2d,
    width: u32,
    height: u32,
}
```

## Window Management

### Creating Windows

```rust
// Via compositor
let window_id = comp.create_window("Terminal", owner_task);

// Via syscall
let fd = syscall::window_create("My App")?;
```

### Window Layout

Windows are tiled automatically:

```
┌───────────────────────────────────────┐
│ Terminal                              │
├───────────────────┬───────────────────┤
│ Files             │ Editor            │
│                   │                   │
│                   │                   │
└───────────────────┴───────────────────┘
```

Layout algorithm:
1. First window fills screen
2. Second window splits horizontally
3. Third window splits the second half
4. Pattern continues recursively

### Focus Management

```rust
// Click handlers set focus
compositor::handle_click(x, y, button);

// Focused window gets input events
if let Some(focused) = comp.focused {
    // Route keyboard events here
}
```

## Rendering

### Render Loop

Called from `requestAnimationFrame`:

```rust
pub fn render() {
    COMPOSITOR.with(|c| {
        let mut comp = c.borrow_mut();
        if let Some(surface) = &comp.surface {
            // Clear
            surface.clear();

            // Draw each window
            for window in &comp.windows {
                draw_window(surface, window);
            }
        }
    });
}
```

### Drawing Windows

```rust
fn draw_window(surface: &Surface, window: &Window) {
    let ctx = &surface.context;

    // Window background
    ctx.set_fill_style(&"#1a1a2e".into());
    ctx.fill_rect(rect.x, rect.y, rect.width, rect.height);

    // Title bar
    ctx.set_fill_style(&"#16213e".into());
    ctx.fill_rect(rect.x, rect.y, rect.width, 24.0);

    // Title text
    ctx.set_fill_style(&"#fff".into());
    ctx.set_font("14px monospace");
    ctx.fill_text(&window.title, rect.x + 8.0, rect.y + 17.0);

    // Border for focused window
    if window.is_focused {
        ctx.set_stroke_style(&"#00ff88".into());
        ctx.stroke_rect(rect.x, rect.y, rect.width, rect.height);
    }
}
```

## Event Handling

### Mouse Events

```rust
pub fn handle_click(x: f64, y: f64, button: i16) {
    COMPOSITOR.with(|c| {
        let mut comp = c.borrow_mut();

        // Find clicked window
        for (i, window) in comp.windows.iter().enumerate() {
            if window.rect.contains(x, y) {
                comp.focused = Some(i);

                // Check if in title bar (for dragging, later)
                if y < window.rect.y + 24.0 {
                    // Title bar click
                }
                break;
            }
        }
    });
}
```

### Resize Events

```rust
pub fn resize(width: u32, height: u32) {
    COMPOSITOR.with(|c| {
        let mut comp = c.borrow_mut();

        if let Some(surface) = &mut comp.surface {
            surface.resize(width, height);
        }

        // Recalculate layout
        comp.layout.set_bounds(Rect {
            x: 0.0,
            y: 0.0,
            width: width as f64,
            height: height as f64,
        });
        comp.update_window_rects();
    });
}
```

## Geometry

### Rect

```rust
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn contains(&self, x: f64, y: f64) -> bool;
    pub fn split_horizontal(&self, ratio: f32) -> (Rect, Rect);
    pub fn split_vertical(&self, ratio: f32) -> (Rect, Rect);
}
```

### Window Content Area

Windows have a title bar (24px) and border (4px):

```rust
impl Window {
    pub fn content_rect(&self) -> Rect {
        Rect {
            x: self.rect.x + 4.0,
            y: self.rect.y + 24.0,
            width: self.rect.width - 8.0,
            height: self.rect.height - 28.0,
        }
    }
}
```

## Initialization

```rust
pub async fn init(&mut self) -> Result<(), String> {
    // Get the canvas element
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id("canvas")
        .ok_or("no canvas")?
        .dyn_into::<HtmlCanvasElement>()?;

    // Get 2D context
    let context = canvas
        .get_context("2d")?
        .ok_or("no 2d context")?
        .dyn_into::<CanvasRenderingContext2d>()?;

    self.surface = Some(Surface::new(canvas, context));
    Ok(())
}
```

## Global Access

```rust
thread_local! {
    pub static COMPOSITOR: RefCell<Compositor> = RefCell::new(Compositor::new());
}

// Render from runtime
pub fn render() {
    COMPOSITOR.with(|c| c.borrow().render());
}

// Handle events
pub fn handle_click(x: f64, y: f64, button: i16) {
    COMPOSITOR.with(|c| c.borrow_mut().handle_click(x, y, button));
}
```

## Related Documentation

- [Syscall Interface](../kernel/syscalls.md) - window_create
- [Executor](../kernel/executor.md) - Render timing
- [Future Work](../future-work.md) - Planned enhancements
