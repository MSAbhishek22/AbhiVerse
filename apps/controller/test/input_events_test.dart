import 'package:abhiverse_controller/core/input_events.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('events use the tagged protocol JSON shapes', () {
    expect(mouseMove(0.5, 0.25), {'type': 'mouse_move', 'x': 0.5, 'y': 0.25});
    expect(mouseDown(2), {'type': 'mouse_down', 'button': 2});
    expect(keyDown('KeyA'), {'type': 'key_down', 'code': 'KeyA'});
    expect(textEvent('hi'), {'type': 'text', 'text': 'hi'});
    expect(scrollEvent(0, -1), {'type': 'scroll', 'dx': 0.0, 'dy': -1.0});
  });

  test('tap sequence is move, down, up with clamped coordinates', () {
    final events = tapSequence(1.4, -0.5);
    expect(events.map((e) => e['type']),
        ['mouse_move', 'mouse_down', 'mouse_up']);
    expect(events.first['x'], 1.0);
    expect(events.first['y'], 0.0);
    expect(tapSequence(0.1, 0.2, button: 2)[1]['button'], 2);
  });

  test('key tap pairs down and up', () {
    expect(keyTap('Enter'), [
      {'type': 'key_down', 'code': 'Enter'},
      {'type': 'key_up', 'code': 'Enter'},
    ]);
  });
}
