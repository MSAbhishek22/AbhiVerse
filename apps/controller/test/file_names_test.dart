import 'package:abhiverse_controller/core/file_names.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('sanitizeDownloadName keeps only the basename', () {
    expect(sanitizeDownloadName('note.txt'), 'note.txt');
    expect(sanitizeDownloadName('a/b/c.txt'), 'c.txt');
    expect(sanitizeDownloadName('../../etc/passwd'), 'passwd');
    expect(
      sanitizeDownloadName(r'..\shared_prefs\FlutterSecureStorage.xml'),
      'FlutterSecureStorage.xml',
    );
    expect(sanitizeDownloadName(''), 'download');
    expect(sanitizeDownloadName('..'), 'download');
    expect(sanitizeDownloadName('/'), 'download');
  });

  test('checkTransferSize enforces the 1 GB cap', () {
    expect(checkTransferSize(1024 * 1024 * 1024), isNull);
    expect(checkTransferSize(1024 * 1024 * 1024 + 1), contains('1 GB'));
  });
}
