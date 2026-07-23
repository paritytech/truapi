use parity_scale_codec::{Compact, Decode, Encode, OptionBool};

/// A size/dimension value (logical pixels) used across the custom renderer.
///
/// Encoded as a SCALE `Compact<u64>`: the common small values cost a single
/// byte on the wire instead of eight.
pub type Size = Compact<u64>;

/// CSS-like dimensions: (top, end, bottom, start).
/// Bottom defaults to top, start defaults to end when `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct Dimensions {
    /// Top dimension.
    pub top: Size,
    /// End dimension.
    pub end: Size,
    /// Bottom dimension. Defaults to top when absent.
    pub bottom: Option<Size>,
    /// Start dimension. Defaults to end when absent.
    pub start: Option<Size>,
}

/// Text typography presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum TypographyStyle {
    /// Large headline text.
    HeadlineLarge,
    /// Medium title text, regular weight.
    TitleMediumRegular,
    /// Large body text, regular weight.
    BodyLargeRegular,
    /// Medium body text, regular weight.
    BodyMediumRegular,
    /// Small body text, regular weight.
    BodySmallRegular,
}

/// Button style variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum ButtonVariant {
    /// Emphasized button for the primary action.
    Primary,
    /// De-emphasized button for secondary actions.
    Secondary,
    /// Text-only button without a background.
    Text,
}

/// Semantic color tokens for theming.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum ColorToken {
    /// Primary foreground (text) color.
    FgPrimary,
    /// Secondary foreground color.
    FgSecondary,
    /// Tertiary foreground color.
    FgTertiary,
    /// Main surface background.
    BgSurfaceMain,
    /// Container surface background.
    BgSurfaceContainer,
    /// Nested surface background.
    BgSurfaceNested,
    /// Foreground color for success states.
    FgSuccess,
    /// Foreground color for error states.
    FgError,
    /// Foreground color for warning states.
    FgWarning,
}

/// 2D content alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum ContentAlignment {
    /// Top edge, start side.
    TopStart,
    /// Top edge, horizontally centered.
    TopCenter,
    /// Top edge, end side.
    TopEnd,
    /// Vertically centered, start side.
    CenterStart,
    /// Centered on both axes.
    Center,
    /// Vertically centered, end side.
    CenterEnd,
    /// Bottom edge, start side.
    BottomStart,
    /// Bottom edge, horizontally centered.
    BottomCenter,
    /// Bottom edge, end side.
    BottomEnd,
}

/// Horizontal alignment options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum HorizontalAlignment {
    /// Align to the start edge.
    Start,
    /// Center horizontally.
    Center,
    /// Align to the end edge.
    End,
}

/// Vertical alignment options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum VerticalAlignment {
    /// Align to the top.
    Top,
    /// Center vertically.
    Center,
    /// Align to the bottom.
    Bottom,
}

/// Layout arrangement (like CSS flexbox `justify-content`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum Arrangement {
    /// Pack children at the start.
    Start,
    /// Pack children at the end.
    End,
    /// Pack children in the center.
    Center,
    /// Distribute with space between children.
    SpaceBetween,
    /// Distribute with space around each child.
    SpaceAround,
    /// Distribute with equal space between and around children.
    SpaceEvenly,
}

/// Shape for borders and backgrounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum Shape {
    /// Border radius value.
    Rounded {
        /// Border radius.
        radius: Size,
    },
    /// Circular shape.
    Circle,
}

/// Border styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct BorderStyle {
    /// Border width.
    pub width: Size,
    /// Border color.
    pub color: ColorToken,
    /// Border shape.
    pub shape: Option<Shape>,
}

/// Background styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct Background {
    /// Background color.
    pub color: ColorToken,
    /// Background shape.
    pub shape: Option<Shape>,
}

/// Layout and styling modifiers applied to custom renderer components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
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
    Height {
        /// Fixed height.
        height: Size,
    },
    /// Fixed width.
    Width {
        /// Fixed width.
        width: Size,
    },
    /// Minimum width.
    MinWidth {
        /// Minimum width.
        width: Size,
    },
    /// Minimum height.
    MinHeight {
        /// Minimum height.
        height: Size,
    },
    /// Fill available width.
    FillWidth {
        /// Whether width should fill available space.
        enabled: bool,
    },
    /// Fill available height.
    FillHeight {
        /// Whether height should fill available space.
        enabled: bool,
    },
}

/// Properties for a [`CustomRendererNode::Box`] container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct BoxProps {
    /// Content alignment within the box.
    pub content_alignment: Option<ContentAlignment>,
}

/// Properties for a [`CustomRendererNode::Column`] layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct ColumnProps {
    /// Horizontal alignment of children.
    pub horizontal_alignment: Option<HorizontalAlignment>,
    /// Vertical arrangement of children.
    pub vertical_arrangement: Option<Arrangement>,
}

/// Properties for a [`CustomRendererNode::Row`] layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct RowProps {
    /// Vertical alignment of children.
    pub vertical_alignment: Option<VerticalAlignment>,
    /// Horizontal arrangement of children.
    pub horizontal_arrangement: Option<Arrangement>,
}

/// Properties for a [`CustomRendererNode::Text`] display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct TextProps {
    /// Typography preset.
    pub style: Option<TypographyStyle>,
    /// Text color.
    pub color: Option<ColorToken>,
}

/// Properties for a [`CustomRendererNode::Button`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ButtonProps {
    /// Button label text.
    pub text: String,
    /// Button style variant.
    pub variant: Option<ButtonVariant>,
    /// Whether the button is enabled. Absent leaves the default to the host.
    pub enabled: OptionBool,
    /// Whether the button shows a loading state. Absent leaves the default to the host.
    pub loading: OptionBool,
    /// Action identifier triggered on click.
    pub click_action: Option<String>,
}

/// Properties for a [`CustomRendererNode::TextField`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TextFieldProps {
    /// Current text value.
    pub text: String,
    /// Placeholder text.
    pub placeholder: Option<String>,
    /// Field label.
    pub label: Option<String>,
    /// Whether the field is enabled. Absent leaves the default to the host.
    pub enabled: OptionBool,
    /// Action identifier triggered when the value changes.
    pub value_change_action: Option<String>,
}

/// A component in the custom renderer UI tree, combining modifiers, typed props,
/// and recursive children.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Component<P> {
    /// Layout and styling modifiers.
    pub modifiers: Vec<Modifier>,
    /// Component-specific properties.
    pub props: P,
    /// Child nodes.
    pub children: Vec<CustomRendererNode>,
}

/// A node in the custom renderer UI tree. Can be nested recursively via the
/// `children` field of each [`Component`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CustomRendererNode {
    /// Empty node.
    Nil,
    /// Raw text string.
    String {
        /// Raw text.
        text: String,
    },
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

/// Subscribe payload identifying the chat message to render. The host responds
/// with a stream of [`CustomRendererNode`] trees describing the rendered UI.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProductChatCustomMessageRenderSubscribeRequest {
    /// Message identifier.
    pub message_id: String,
    /// Application-defined message type.
    pub message_type: String,
    /// Binary payload.
    pub payload: Vec<u8>,
}
