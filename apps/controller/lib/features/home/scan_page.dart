import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import '../../core/pairing_code.dart';

/// Full-screen QR scanner for pairing codes. Pops with the parsed
/// 6-digit code, or null if the user backs out.
class ScanPairingPage extends StatefulWidget {
  const ScanPairingPage({super.key});

  @override
  State<ScanPairingPage> createState() => _ScanPairingPageState();
}

class _ScanPairingPageState extends State<ScanPairingPage> {
  bool _handled = false;

  void _onDetect(BarcodeCapture capture) {
    if (_handled) return;
    for (final barcode in capture.barcodes) {
      final raw = barcode.rawValue;
      if (raw == null) continue;
      final code = parsePairingCode(raw);
      if (code != null) {
        _handled = true;
        Navigator.of(context).pop(code);
        return;
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Scan pairing QR code')),
      body: MobileScanner(onDetect: _onDetect),
    );
  }
}
