import 'package:flutter/material.dart';

/// One heading + body block inside a doc screen (Terms, Security, FAQ).
class DocSection {
  const DocSection(this.heading, this.body);

  final String heading;
  final String body;
}

/// A plain scrollable document screen: title up top, sections below.
/// No markdown dependency, no web view — just readable text, so it
/// works offline and never phones home to render.
class DocScreen extends StatelessWidget {
  const DocScreen({super.key, required this.title, required this.sections});

  final String title;
  final List<DocSection> sections;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: Text(title)),
      body: ListView(
        padding: const EdgeInsets.all(24),
        children: [
          for (final section in sections) ...[
            Text(
              section.heading,
              style: Theme.of(context).textTheme.titleMedium,
            ),
            const SizedBox(height: 4),
            Text(section.body),
            const SizedBox(height: 20),
          ],
        ],
      ),
    );
  }
}
