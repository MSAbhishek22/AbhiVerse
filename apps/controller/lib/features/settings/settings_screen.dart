import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../core/app_info.dart';
import '../../core/endpoint_security.dart';
import '../legal/legal_docs.dart';
import 'doc_screen.dart';

/// The Settings hub: server, docs, support, about.
///
/// The server section is the old "Server settings" dialog, moved here
/// so everything the user is supposed to find lives in one place:
/// server, Terms, Privacy & Security, FAQ, licences, support, version.
/// Pops `true` when the server URL was changed, so the home screen
/// knows to reconnect; otherwise pops nothing.
class SettingsScreen extends StatefulWidget {
  const SettingsScreen({
    super.key,
    required this.currentServerUrl,
    required this.onServerChanged,
  });

  /// The server URL the app is currently pointed at.
  final String currentServerUrl;

  /// Called with the new URL, or null to reset to the build default.
  /// The caller owns persistence + rebuilding its API clients.
  final Future<void> Function(String?) onServerChanged;

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  /// Null means "default server (build setting)" after a reset.
  String? _url;
  var _changed = false;

  @override
  void initState() {
    super.initState();
    _url = widget.currentServerUrl;
  }

  String get _displayUrl => _url ?? 'Default server (build setting)';

  Future<void> _showServerDialog() async {
    final urlController = TextEditingController(text: _url ?? '');
    var error = '';
    final result = await showDialog<String?>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (context, setDialogState) => AlertDialog(
          title: const Text('Server settings'),
          content: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text(
                  'The computer running the Abhiverse server. On your '
                  'home Wi-Fi this is its LAN address, e.g. '
                  'http://192.168.1.20:8080. A hosted server must use '
                  'https.',
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: urlController,
                  keyboardType: TextInputType.url,
                  decoration: const InputDecoration(
                    labelText: 'Server URL',
                    hintText: 'http://192.168.1.20:8080',
                  ),
                ),
                if (error.isNotEmpty) ...[
                  const SizedBox(height: 8),
                  Text(error, style: const TextStyle(color: Colors.red)),
                ],
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext),
              child: const Text('Cancel'),
            ),
            TextButton(
              onPressed: () => Navigator.pop(dialogContext, ''),
              child: const Text('Reset to default'),
            ),
            ElevatedButton(
              onPressed: () {
                final check = checkApiBaseUrl(urlController.text.trim());
                if (check.blocked) {
                  setDialogState(() => error = check.message ?? 'Bad URL.');
                  return;
                }
                Navigator.pop(dialogContext, urlController.text.trim());
              },
              child: const Text('Save'),
            ),
          ],
        ),
      ),
    );
    // Cancel/backdrop pops null (do nothing); Reset pops '' and
    // Save pops the URL string.
    if (result == null || !mounted) return;
    if (result.isEmpty) {
      await widget.onServerChanged(null);
      if (!mounted) return;
      setState(() {
        _url = null;
        _changed = true;
      });
    } else {
      await widget.onServerChanged(result);
      if (!mounted) return;
      setState(() {
        _url = result;
        _changed = true;
      });
    }
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(content: Text('Server changed — reconnecting…')),
    );
  }

  void _openDoc(String title, List<DocSection> sections) {
    Navigator.push(
      context,
      MaterialPageRoute(
        builder: (_) => DocScreen(title: title, sections: sections),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    // Every way out of Settings (back button, system back, swipe)
    // reports whether the server changed, so the home screen knows
    // to reconnect.
    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, _) {
        if (didPop) return;
        Navigator.pop(context, _changed ? true : null);
      },
      child: Scaffold(
        appBar: AppBar(title: const Text('Settings')),
        body: ListView(
          padding: const EdgeInsets.all(24),
          children: [
            Text(
              'Server',
              style: Theme.of(context).textTheme.titleMedium,
            ),
            const SizedBox(height: 8),
            Text(_displayUrl),
            const SizedBox(height: 8),
            OutlinedButton(
              onPressed: _showServerDialog,
              child: const Text('Change server'),
            ),
            const SizedBox(height: 24),
            Text(
              'Legal & docs',
              style: Theme.of(context).textTheme.titleMedium,
            ),
            const SizedBox(height: 8),
            ListTile(
              leading: const Icon(Icons.description),
              title: const Text('Terms & Conditions'),
              subtitle: const Text('The deal, the blunt part, the limits'),
              onTap: () => _openDoc('Terms & Conditions', termsSections),
            ),
            ListTile(
              leading: const Icon(Icons.shield),
              title: const Text('Privacy & Security'),
              subtitle: const Text('How it protects you, honestly'),
              onTap: () => _openDoc('Privacy & Security', securitySections),
            ),
            ListTile(
              leading: const Icon(Icons.help),
              title: const Text('FAQ'),
              subtitle: const Text('Questions everyone actually asks'),
              onTap: () => _openDoc('FAQ', faqSections),
            ),
            ListTile(
              leading: const Icon(Icons.balance),
              title: const Text('Open-source licences'),
              subtitle: const Text('Every package this app stands on'),
              onTap: () => showLicensePage(
                context: context,
                applicationName: 'ABHIVERSE',
                applicationVersion: kAppVersion,
              ),
            ),
            const SizedBox(height: 24),
            Text(
              'Support',
              style: Theme.of(context).textTheme.titleMedium,
            ),
            const SizedBox(height: 8),
            const Text(
              'Made with love by M S Abhishek. Abhiverse is free '
              'forever — if it saves you time and you feel like giving '
              'back, here is the UPI ID. Zero pressure, zero paywall.',
            ),
            const SizedBox(height: 8),
            Row(
              children: [
                Expanded(child: Text(kSupportUpiId)),
                TextButton.icon(
                  icon: const Icon(Icons.copy, size: 18),
                  label: const Text('Copy'),
                  onPressed: () async {
                    await Clipboard.setData(
                      const ClipboardData(text: kSupportUpiId),
                    );
                    if (context.mounted) {
                      ScaffoldMessenger.of(context).showSnackBar(
                        const SnackBar(
                          content: Text('UPI ID copied. Thank you!'),
                        ),
                      );
                    }
                  },
                ),
              ],
            ),
            const SizedBox(height: 24),
            Text(
              'About',
              style: Theme.of(context).textTheme.titleMedium,
            ),
            const SizedBox(height: 8),
            Text('ABHIVERSE version $kAppVersion'),
            const Text(
              'Free & open source (MIT) · No account · No subscription',
            ),
            Text(kRepoUrl),
            const SizedBox(height: 8),
            const Text(
              'Beta: real-device testing is still in progress. If '
              'something breaks, that is what the repo issues are for.',
            ),
          ],
        ),
      ),
    );
  }
}
