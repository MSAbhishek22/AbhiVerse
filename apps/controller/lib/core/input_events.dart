/// Builders for the protocol `InputEvent` JSON vocabulary — the exact
/// tagged shapes the Rust protocol crate defines
/// (`{"type":"mouse_move","x":0.5,"y":0.25}`) and the agent injects
/// when started with `--allow-input` (Task 006c).
library;

Map<String, dynamic> mouseMove(double x, double y) => {
      'type': 'mouse_move',
      'x': x.clamp(0.0, 1.0),
      'y': y.clamp(0.0, 1.0),
    };

Map<String, dynamic> mouseDown(int button) =>
    {'type': 'mouse_down', 'button': button};

Map<String, dynamic> mouseUp(int button) =>
    {'type': 'mouse_up', 'button': button};

Map<String, dynamic> scrollEvent(double dx, double dy) =>
    {'type': 'scroll', 'dx': dx, 'dy': dy};

Map<String, dynamic> keyDown(String code) =>
    {'type': 'key_down', 'code': code};

Map<String, dynamic> keyUp(String code) => {'type': 'key_up', 'code': code};

Map<String, dynamic> textEvent(String text) => {'type': 'text', 'text': text};

/// A full click at a normalised position: move, press, release.
/// Buttons: 0 = left, 1 = middle, 2 = right.
List<Map<String, dynamic>> tapSequence(double x, double y, {int button = 0}) =>
    [mouseMove(x, y), mouseDown(button), mouseUp(button)];

/// A key tap: press + release for a web `KeyboardEvent.code`.
List<Map<String, dynamic>> keyTap(String code) => [keyDown(code), keyUp(code)];
