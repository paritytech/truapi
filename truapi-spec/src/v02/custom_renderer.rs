use super::*;

/// Variable-length unsigned integer used for dimensions (SCALE compact-encoded
/// on the wire).
pub type Size = u64;

/// CSS-like dimensions: (top, end, bottom, start).
/// Bottom defaults to top, start defaults to end when `None`.
pub type Dimensions = (Size, Size, Option<Size>, Option<Size>);

/// Text typography presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypographyStyle {
    TitleXL,
    Headline,
    BodyM,
    BodyS,
    Caption,
}

/// Button style variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Text,
}

/// Semantic color tokens for theming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorToken {
    TextPrimary,
    TextSecondary,
    TextTertiary,
    BackgroundPrimary,
    BackgroundSecondary,
    BackgroundTertiary,
    Success,
    Error,
    Warning,
}

/// 2D content alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentAlignment {
    TopStart,
    TopCenter,
    TopEnd,
    CenterStart,
    Center,
    CenterEnd,
    BottomStart,
    BottomCenter,
    BottomEnd,
}

/// Horizontal alignment options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorizontalAlignment {
    Start,
    Center,
    End,
}

/// Vertical alignment options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalAlignment {
    Top,
    Center,
    Bottom,
}

/// Layout arrangement (like CSS flexbox `justify-content`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrangement {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Shape for borders and backgrounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Border radius value.
    Rounded(Size),
    /// Circular shape.
    Circle,
}

/// Border styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BorderStyle {
    /// Border width.
    pub width: Size,
    /// Border color.
    pub color: ColorToken,
    /// Border shape.
    pub shape: Option<Shape>,
}

/// Background styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Background {
    /// Background color.
    pub color: ColorToken,
    /// Background shape.
    pub shape: Option<Shape>,
}

/// Layout and styling modifiers applied to custom renderer components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    /// Outer spacing.
    Margin(Dimensions),
    /// Inner spacing.
    Padding(Dimensions),
    /// Background fill.
    Background(Background),
    /// Border style.
    Border(BorderStyle),
    /// Fixed height.
    Height(Size),
    /// Fixed width.
    Width(Size),
    /// Minimum width.
    MinWidth(Size),
    /// Minimum height.
    MinHeight(Size),
    /// Fill available width.
    FillWidth(bool),
    /// Fill available height.
    FillHeight(bool),
}

/// Properties for a [`CustomRendererNode::Box`] container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxProps {
    /// Content alignment within the box.
    pub content_alignment: Option<ContentAlignment>,
}

/// Properties for a [`CustomRendererNode::Column`] layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnProps {
    /// Horizontal alignment of children.
    pub horizontal_alignment: Option<HorizontalAlignment>,
    /// Vertical arrangement of children.
    pub vertical_arrangement: Option<Arrangement>,
}

/// Properties for a [`CustomRendererNode::Row`] layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowProps {
    /// Vertical alignment of children.
    pub vertical_alignment: Option<VerticalAlignment>,
    /// Horizontal arrangement of children.
    pub horizontal_arrangement: Option<Arrangement>,
}

/// Properties for a [`CustomRendererNode::Text`] display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextProps {
    /// Typography preset.
    pub style: Option<TypographyStyle>,
    /// Text color.
    pub color: Option<ColorToken>,
}

/// Properties for a [`CustomRendererNode::Button`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButtonProps {
    /// Button label text.
    pub text: String,
    /// Button style variant.
    pub variant: Option<ButtonVariant>,
    /// Action identifier triggered on click.
    pub click_action: String,
}

/// Properties for a [`CustomRendererNode::TextField`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFieldProps {
    /// Placeholder text.
    pub placeholder: Option<String>,
    /// Initial value.
    pub initial_value: Option<String>,
    /// Action identifier triggered on submit.
    pub submit_action: String,
}

/// A node in the custom renderer UI tree. Can be nested recursively via the
/// `children` field of each [`Component`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomRendererNode {
    /// Empty node.
    Nil,
    /// Raw text string.
    String(String),
    /// Generic container.
    Box(Component<BoxProps>),
    /// Vertical layout.
    Column(Component<ColumnProps>),
    /// Horizontal layout.
    Row(Component<RowProps>),
    /// Flexible space.
    Spacer(Component<()>),
    /// Text display.
    Text(Component<TextProps>),
    /// Interactive button.
    Button(Component<ButtonProps>),
    /// Text input.
    TextField(Component<TextFieldProps>),
}

/// A component in the custom renderer UI tree, combining modifiers, typed props,
/// and recursive children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component<P> {
    /// Layout and styling modifiers.
    pub modifiers: Vec<Modifier>,
    /// Component-specific properties.
    pub props: P,
    /// Child nodes.
    pub children: Vec<CustomRendererNode>,
}

/// Request from the host asking the product to render a custom chat message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomMessageRenderRequest {
    /// Message identifier.
    pub message_id: String,
    /// Application-defined message type.
    pub message_type: String,
    /// Binary payload.
    pub payload: Bytes,
}
