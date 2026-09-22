---
name: flutter
description: "AI Agent Skill for Flutter — design cross-platform mobile and web applications with Flutter. Focuses on widget structure, state management, layouts, navigation, and performance."
metadata:
  version: 1.0.0
---

# Flutter AI Agent Skill 📱

You are a Flutter development specialist. Follow these rules and practices.

## Core Rules & Guidelines

1. **Declarative UI**: Build UI as functions of state. Keep widgets small and single-purpose. Break down complex build methods into smaller widgets.
2. **State Management**: Use `Riverpod` (specifically `NotifierProvider` and `AsyncNotifierProvider`) for global/shared state. Do not use `setState` for anything beyond local/ephemeral state.
3. **Const Constructors**: Use `const` prefix wherever possible to optimize rendering performance by cached widget trees.
4. **Adaptive & Responsive**: Use `LayoutBuilder`, `MediaQuery`, or package helpers to ensure layout works perfectly across mobile, tablet, and desktop screens.

## Common Patterns

### Riverpod ConsumerWidget
```dart
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

// Simple Riverpod state notifier
class CounterNotifier extends AutoDisposeNotifier<int> {
  @override
  int build() => 0;

  void increment() => state++;
}

final counterProvider = NotifierProvider.autoDispose<CounterNotifier, int>(CounterNotifier.new);

class CounterScreen extends ConsumerWidget {
  const CounterScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final count = ref.watch(counterProvider);

    return Scaffold(
      appBar: AppBar(title: const Text('Counter')),
      body: Center(
        child: Text(
          '$count',
          style: Theme.of(context).textTheme.headlineLarge,
        ),
      ),
      floatingActionButton: FloatingActionButton(
        onPressed: () => ref.read(counterProvider.notifier).increment(),
        child: const Icon(Icons.add),
      ),
    );
  }
}
```
