use iced::advanced::{Clipboard, Shell, Widget, layout, mouse, renderer, widget::Tree};
use iced::widget::button;
use iced::{Element, Event, Length, Point, Rectangle, Size, touch};

pub struct ListItem<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    on_press: Option<Message>,
    style: fn(&Theme, button::Status) -> button::Style,
}

impl<'a, Message, Theme, Renderer> ListItem<'a, Message, Theme, Renderer> {
    pub fn new(content: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        Self {
            content: content.into(),
            on_press: None,
            // Provide a default fallback style
            style: |_theme, _status| button::Style::default(),
        }
    }

    pub fn on_press(mut self, msg: Message) -> Self {
        self.on_press = Some(msg);
        self
    }

    // Add a builder method to accept your theme function
    pub fn style(mut self, style: fn(&Theme, button::Status) -> button::Style) -> Self {
        self.style = style;
        self
    }
}

#[derive(Default)]
struct State {
    is_pressed: bool,
    is_hovered: bool,
    start_pos: Option<Point>,
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for ListItem<'a, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> iced::advanced::widget::tree::Tag {
        iced::advanced::widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> iced::advanced::widget::tree::State {
        iced::advanced::widget::tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content))
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: layout::Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        // Delegate to child first
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );

        if self.on_press.is_none() {
            return;
        }

        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();

        let is_mouse_over = cursor.is_over(bounds);
        if is_mouse_over != state.is_hovered {
            state.is_hovered = is_mouse_over;
            shell.request_redraw();
        }

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                if let Some(cursor_position) = cursor.position_over(bounds) {
                    state.is_pressed = true;
                    state.start_pos = Some(cursor_position);
                    // Do NOT capture event to allow Scrollable to use it
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerLifted { .. }) => {
                if state.is_pressed {
                    state.is_pressed = false;

                    if let Some(cursor_position) = cursor.position_over(bounds) {
                        if let Some(start) = state.start_pos {
                            if start.distance(cursor_position) < 15.0 {
                                shell.publish(self.on_press.clone().unwrap());
                            }
                        }
                    }
                    state.start_pos = None;
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. })
            | Event::Touch(touch::Event::FingerMoved { .. }) => {
                if state.is_pressed {
                    if let Some(cursor_position) = cursor.position() {
                        if let Some(start) = state.start_pos {
                            if start.distance(cursor_position) > 15.0 {
                                state.is_pressed = false;
                                state.start_pos = None;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: layout::Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<State>();
        let is_mouse_over = cursor.is_over(bounds);

        // 1. Determine the current button status
        let status = if self.on_press.is_none() {
            button::Status::Disabled
        } else if state.is_pressed {
            button::Status::Pressed
        } else if is_mouse_over {
            button::Status::Hovered
        } else {
            button::Status::Active
        };

        // 2. Fetch the style from your custom theme function
        let appearance = (self.style)(theme, status);

        // 3. Draw the background and borders first
        if let Some(background) = appearance.background {
            renderer.fill_quad(
                iced::advanced::renderer::Quad {
                    bounds,
                    border: appearance.border,
                    shadow: appearance.shadow,
                    snap: true,
                },
                background,
            );
        }

        // 4. Update the text color to cascade down to the text widgets
        let mut child_style = *style;
        child_style.text_color = appearance.text_color;

        // 5. Draw the children over the background using the updated style
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            &child_style,
            layout,
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: layout::Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let child_interaction = self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        );

        if child_interaction != mouse::Interaction::default() {
            return child_interaction;
        }

        if cursor.is_over(layout.bounds()) && self.on_press.is_some() {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

impl<'a, Message, Theme, Renderer> From<ListItem<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a + Clone,
    Theme: 'a,
    Renderer: 'a + renderer::Renderer,
{
    fn from(list_item: ListItem<'a, Message, Theme, Renderer>) -> Self {
        Self::new(list_item)
    }
}
