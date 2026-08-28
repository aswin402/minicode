#![allow(clippy::all, unused)]

use crate::tools::onpkg::stacks::{Stack, StackFile};

pub fn flutter_riverpod(app_name: Option<&str>) -> Stack {
    let name = app_name.unwrap_or("my_app").to_string();

    let title = app_name
        .map(|s| {
            let mut t = s.to_string();
            if let Some(first) = t.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            t
        })
        .unwrap_or_else(|| "Onpkg_flutter-riverpod".to_string());

    Stack {
        hooks: vec![],
        name: if name.is_empty() { "flutter-riverpod".into() } else { format!("flutter-riverpod-{}", name).into() },
        runtime: "flutter".into(),
        description: "Flutter + Riverpod/HooksRiverpod + GoRouter + Dio + Material 3 (Custom Extensions) + Logger + Google Fonts".into(),
        packages: vec![
            "google_fonts".into(),
            "logger".into(),
            "flutter_riverpod".into(),
            "go_router".into(),
            "hooks_riverpod".into(),
            "dio".into(),
        ],
        dev_packages: vec![],
        transitive_packages: vec![],
        files: vec![
            StackFile {
                path: "ONPKG_README.md".into(),
                content: r##"# Onpkg Flutter App

A modern, scalable Flutter application serving as the client for Onpkg.

## Key Technologies

- **State Management**: Riverpod (`hooks_riverpod`, `flutter_riverpod`)
- **Routing**: GoRouter
- **Networking**: Dio
- **Typography**: Google Fonts
- **Logging**: Logger

---

## Architecture & Folder Structure

The application follows a **feature-first layered architecture**. Code is organized by features and then by layers within each feature.

```text
lib/
├── core/                  # Core infrastructure and global configurations
│   ├── extension/         # Dart & Flutter extension methods
│   ├── layout/            # Global layout constraints and helpers
│   ├── logger/            # Structured logging setup
│   ├── networks/          # Dio client, API endpoints, and network providers
│   ├── router/            # GoRouter configuration
│   └── theme/             # App themes (Light/Dark) and theme provider
├── features/              # Feature modules
│   └── home/
│       └── presentation/  # UI widgets, screens, and state controllers
├── shared/                # Shared components across the app
│   └── widgets/           # Reusable UI components
└── main.dart              # Application entry point
```

### Layer Responsibilities

- **`core/`** — Global infrastructure used across the entire application (networking, routing, theming, logging, etc.).
- **`features/`** — Self-contained business features. Each major capability (e.g., `home`, `auth`, `settings`) resides in its own directory.
- **`shared/`** — Reusable UI components used across multiple features.

---

## Usage

This project uses `just` as the command runner for common development tasks.

| Command       | Description                                      |
|---------------|--------------------------------------------------|
| `just dev`    | Run the app on Chrome (`flutter run -d chrome`)  |
| `just run`    | Run the app on the default device                |
| `just scan`   | List connected devices (`flutter devices`)       |
| `just analyze`| Run static analysis and Flutter doctor           |

You can also run standard Flutter commands directly.

---

## How to Modify

### Adding a New Feature

1. Create a new folder under `lib/features/` (example: `lib/features/settings/`).
2. Add the required sub-folders: `presentation/`, `data/`, and `domain/` as needed.
3. Implement your UI screens and Riverpod providers within the feature folder.

### Adding a New Route

1. Add the route path and name constants in `lib/core/router/app_routes.dart`.
2. Update `lib/core/router/app_router.dart` and add the new `GoRoute` definition.

### Adding a New API Call

1. Define the endpoint in `lib/core/networks/api_endpoints.dart`.
2. Use the existing Dio client provider from `lib/core/networks/network_provider.dart` in your feature's repository or provider.

### Modifying the Theme

1. Update `lib/core/theme/app_themes.dart`.
2. Modify the `ThemeData` for light or dark mode.
3. Adjust Google Fonts text styles for global typography changes.

---

## Contributing

- Follow the established feature-first architecture.
- Run `just analyze` before committing to ensure code quality.
- Place reusable widgets in `lib/shared/widgets/` when they are used across multiple features.
```
"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "analysis_options.yaml".into(),
                content: r##"# This file configures the analyzer, which statically analyzes Dart code to
# check for errors, warnings, and lints.
#
# The issues identified by the analyzer are surfaced in the UI of Dart-enabled
# IDEs (https://dart.dev/tools#ides-and-editors). The analyzer can also be
# invoked from the command line by running `flutter analyze`.

# The following line activates a set of recommended lints for Flutter apps,
# packages, and plugins designed to encourage good coding practices.
include: package:flutter_lints/flutter.yaml

linter:
  # The lint rules applied to this project can be customized in the
  # section below to disable rules from the `package:flutter_lints/flutter.yaml`
  # included above or to enable additional rules. A list of all available lints
  # and their documentation is published at https://dart.dev/lints.
  #
  # Instead of disabling a lint rule for the entire project in the
  # section below, it can also be suppressed for a single line of code
  # or a specific dart file by using the `// ignore: name_of_lint` and
  # `// ignore_for_file: name_of_lint` syntax on the line or in the file
  # producing the lint.
  rules:
    # avoid_print: false  # Uncomment to disable the `avoid_print` rule
    # prefer_single_quotes: true  # Uncomment to enable the `prefer_single_quotes` rule

# Additional information about this file can be found at
# https://dart.dev/guides/language/analysis-options
"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "justfile".into(),
                content: r##"scan:
    flutter devices
analyze:
       flutter_doctor . --verbose
dev:
   flutter run -d chrome
run:
   flutter run "##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/extension/context_extension.dart".into(),
                content: r##"import 'package:flutter/material.dart';
import '../theme/app_color_extension.dart';

extension ThemeContext on BuildContext {
  ThemeData get theme => Theme.of(this);
  TextTheme get textTheme => theme.textTheme;
  ColorScheme get colors => theme.colorScheme;
  ColorScheme get cs => colors;
  AppColorExtension get ac => theme.extension<AppColorExtension>()!;
}
"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/layout/app_shell.dart".into(),
                content: r##"import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../extension/context_extension.dart';
import '../../shared/widgets/app_footer/app_footer.dart';
import '../../shared/widgets/app_top_navbar/app_top_navbar.dart';

class AppShell extends ConsumerWidget {
  final Widget child;
  const AppShell({super.key, required this.child});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // Determine if we are in dark mode to set system icon colors correctly
    final isDark = context.theme.brightness == Brightness.dark;

    return AnnotatedRegion<SystemUiOverlayStyle>(
      value: SystemUiOverlayStyle(
        // Makes the status bar transparent and adjusts icon contrast
        statusBarColor: Colors.transparent,
        statusBarIconBrightness: isDark ? Brightness.light : Brightness.dark,
        statusBarBrightness: isDark ? Brightness.dark : Brightness.light,

        // Matches the physical bottom navigation bar of the phone to your app theme
        systemNavigationBarColor: context.theme.scaffoldBackgroundColor,
        systemNavigationBarIconBrightness: isDark ? Brightness.light : Brightness.dark,
        systemNavigationBarDividerColor: Colors.transparent,
      ),
      child: Scaffold(
        // Using your custom Top Navbar
        appBar: const AppTopNavbar(),

        // The 'child' is the page content injected by GoRouter
        body: SafeArea(
          top: false, // AppBar handles the top safe area
          child: SelectionArea(
            child: child,
          ),
        ),

        // Using your custom Footer
        bottomNavigationBar: const AppFooter(),

        // Optional: Ensure the background matches your theme exactly
        backgroundColor: context.theme.scaffoldBackgroundColor,
      ),
    );
  }
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/logger/app_logger.dart".into(),
                content: r##"import 'package:flutter/foundation.dart';
import 'package:logger/logger.dart';

class AppLogger {
  static final Logger _logger = Logger(
    printer: PrettyPrinter(
      methodCount: 1,
      errorMethodCount: 8,
      lineLength: 120,
      colors: true,
      printEmojis: true,
      dateTimeFormat: DateTimeFormat.onlyTimeAndSinceStart,
    ),
    level: kReleaseMode ? Level.warning : Level.debug,
  );

  static void debug(String message) => _logger.d(message);
  static void info(String message) => _logger.i(message);
  static void warning(String message) => _logger.w(message);
  static void error(String message, [dynamic error, StackTrace? stackTrace]) =>
      _logger.e(message, error: error, stackTrace: stackTrace);
}
"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/networks/api_endpoints.dart".into(),
                content: r##"class ApiEndpoints {
  static const String baseUrl = "https://api.example.com"; // Replace with your URL
  static const int receiveTimeout = 15000;
  static const int connectionTimeout = 15000;

  // Features
  static const String login = "/auth/login";
  static const String products = "/products";
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/networks/dio_client.dart".into(),
                content: r##"import 'package:dio/dio.dart';
import '../logger/app_logger.dart';
import 'api_endpoints.dart';
import 'network_exceptions.dart';

class DioClient {
  final Dio _dio;

  DioClient(this._dio) {
    _dio
      ..options.baseUrl = ApiEndpoints.baseUrl
      ..options.connectTimeout = const Duration(milliseconds: ApiEndpoints.connectionTimeout)
      ..options.receiveTimeout = const Duration(milliseconds: ApiEndpoints.receiveTimeout)
      ..options.responseType = ResponseType.json
      ..interceptors.add(
        LogInterceptor(
          requestBody: true,
          responseBody: true,
          // Using debug level for standard logs to avoid cluttering info logs
          logPrint: (obj) => AppLogger.debug(obj.toString()),
        ),
      );
  }

  // GET Method
  Future<Response> get(
    String url, {
    Map<String, dynamic>? queryParameters,
    Options? options,
    CancelToken? cancelToken,
  }) async {
    try {
      return await _dio.get(
        url,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
      );
    } on DioException catch (e) {
      throw NetworkException.fromDioError(e);
    } catch (e) {
      throw _handleUnexpectedError(e);
    }
  }

  // POST Method
  Future<Response> post(
    String url, {
    dynamic data,
    Map<String, dynamic>? queryParameters,
    Options? options,
    CancelToken? cancelToken,
  }) async {
    try {
      return await _dio.post(
        url,
        data: data,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
      );
    } on DioException catch (e) {
      throw NetworkException.fromDioError(e);
    } catch (e) {
      throw _handleUnexpectedError(e);
    }
  }

  // PUT Method
  Future<Response> put(
    String url, {
    dynamic data,
    Map<String, dynamic>? queryParameters,
    Options? options,
    CancelToken? cancelToken,
  }) async {
    try {
      return await _dio.put(
        url,
        data: data,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
      );
    } on DioException catch (e) {
      throw NetworkException.fromDioError(e);
    } catch (e) {
      throw _handleUnexpectedError(e);
    }
  }

  // DELETE Method
  Future<Response> delete(
    String url, {
    dynamic data,
    Map<String, dynamic>? queryParameters,
    Options? options,
    CancelToken? cancelToken,
  }) async {
    try {
      return await _dio.delete(
        url,
        data: data,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
      );
    } on DioException catch (e) {
      throw NetworkException.fromDioError(e);
    } catch (e) {
      throw _handleUnexpectedError(e);
    }
  }

  // Private helper for non-dio errors
  NetworkException _handleUnexpectedError(dynamic e) {
    AppLogger.error("Unexpected Error in DioClient: $e");
    return NetworkException(message: "An unexpected error occurred");
  }
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/networks/network_exceptions.dart".into(),
                content: r##"import 'package:dio/dio.dart';
import '../logger/app_logger.dart';

class NetworkException implements Exception {
  final String message;
  final int? statusCode;

  NetworkException({required this.message, this.statusCode});

  @override
  String toString() => message;

  /// Main factory to handle Dio errors
  factory NetworkException.fromDioError(DioException dioException) {
    String message = "An unexpected error occurred";
    int? statusCode = dioException.response?.statusCode;

    switch (dioException.type) {
      case DioExceptionType.cancel:
        message = "Request to API server was cancelled";
        break;
      case DioExceptionType.connectionTimeout:
        message = "Connection timeout with API server";
        break;
      case DioExceptionType.receiveTimeout:
        message = "Receive timeout in connection with API server";
        break;
      case DioExceptionType.sendTimeout:
        message = "Send timeout in connection with API server";
        break;
      case DioExceptionType.connectionError:
        message = "No internet connection";
        break;
      case DioExceptionType.badResponse:
        message = _handleError(statusCode, dioException.response?.data);
        break;
      case DioExceptionType.unknown:
      default:
        message = "Something went wrong";
        break;
    }

    // Log the error using your AppLogger
    AppLogger.error(
      "Network Error [$statusCode]: $message",
      dioException.error,
      dioException.stackTrace,
    );

    return NetworkException(message: message, statusCode: statusCode);
  }

  static String _handleError(int? statusCode, dynamic error) {
    switch (statusCode) {
      case 400:
        return error?['message'] ?? 'Bad request';
      case 401:
        return 'Unauthorized - Please login again';
      case 403:
        return 'Forbidden - You do not have permission';
      case 404:
        return 'The requested resource was not found';
      case 500:
        return 'Internal server error - Please try again later';
      case 502:
        return 'Bad gateway';
      default:
        return 'Oops! Something went wrong';
    }
  }
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/networks/network_provider.dart".into(),
                content: r##"import 'package:dio/dio.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'dio_client.dart';

final dioProvider = Provider<Dio>((ref) => Dio());

final dioClientProvider = Provider<DioClient>((ref) {
  final dio = ref.watch(dioProvider);
  return DioClient(dio);
});


/*
How to use it in a Feature

When you build a feature (e.g., features/home), your Repository will request the dioClientProvider:
// inside features/home/data/repositories/product_repository.dart
final productRepositoryProvider = Provider((ref) {
  final client = ref.watch(dioClientProvider);
  return ProductRepository(client);
});

class ProductRepository {
  final DioClient _client;
  ProductRepository(this._client);

  Future<void> fetchProducts() async {
    final response = await _client.get(ApiEndpoints.products);
    // Handle response...
  }
}
*/"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/router/app_router.dart".into(),
                content: r##"import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import '../../features/home/presentation/pages/home_page.dart';
import '../layout/app_shell.dart';
import 'app_routes.dart';

final routerProvider = Provider<GoRouter>((ref) {
  return GoRouter(
    initialLocation: AppRoutes.home,
    routes: [
      ShellRoute(
        builder: (context, state, child) => AppShell(child: child),
        routes: [
          GoRoute(
            path: AppRoutes.home,
            builder: (context, state) => const HomePage(),
          ),
          // Add more routes here as you create features
        ],
      ),
    ],
  );
});
"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/router/app_routes.dart".into(),
                content: r##"class AppRoutes {
  static const home = '/';
  // Example: static const profile = '/profile';
}
"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/theme/app_color_extension.dart".into(),
                content: r##"import 'package:flutter/material.dart';
import 'app_colors.dart';

@immutable
class AppColorExtension extends ThemeExtension<AppColorExtension> {

  //Card ==========================================================
  final Color card;
  final Color cardForeground;

  //Navbar================================================
  final Color navbar;

  //Input================================================
  final Color inputBackground;
  final Color inputBorder;

  //Button=====================================================
  final Color buttonBackground;
  final Color buttonForeground;
  final Color secondaryButtonBackground;
  final Color secondaryButtonForeground;

  //Misc =====================================================
  final Color muted;
  final Color mutedForeground;
  final Color border;

  const AppColorExtension({
    required this.card,
    required this.cardForeground,
    required this.navbar,
    required this.inputBackground,
    required this.inputBorder,
    required this.buttonBackground,
    required this.buttonForeground,
    required this.secondaryButtonBackground,
    required this.secondaryButtonForeground,
    required this.muted,
    required this.mutedForeground,
    required this.border,
  });

  //Presets =========================================================

  static const light = AppColorExtension(
    card:                       LightColors.card,
    cardForeground:             LightColors.cardForeground,
    navbar:                     LightColors.navbar,
    inputBackground:            LightColors.inputBackground,
    inputBorder:                LightColors.inputBorder,
    buttonBackground:           LightColors.buttonBackground,
    buttonForeground:           LightColors.buttonForeground,
    secondaryButtonBackground:  LightColors.secondaryButtonBackground,
    secondaryButtonForeground:  LightColors.secondaryButtonForeground,
    muted:                      LightColors.muted,
    mutedForeground:            LightColors.mutedForeground,
    border:                     LightColors.border,
  );

  static const dark = AppColorExtension(
    card:                       DarkColors.card,
    cardForeground:             DarkColors.cardForeground,
    navbar:                     DarkColors.navbar,
    inputBackground:            DarkColors.inputBackground,
    inputBorder:                DarkColors.inputBorder,
    buttonBackground:           DarkColors.buttonBackground,
    buttonForeground:           DarkColors.buttonForeground,
    secondaryButtonBackground:  DarkColors.secondaryButtonBackground,
    secondaryButtonForeground:  DarkColors.secondaryButtonForeground,
    muted:                      DarkColors.muted,
    mutedForeground:            DarkColors.mutedForeground,
    border:                     DarkColors.border,
  );

  //Required overrides =============================================

  @override
  AppColorExtension copyWith({
    Color? card,
    Color? cardForeground,
    Color? navbar,
    Color? inputBackground,
    Color? inputBorder,
    Color? buttonBackground,
    Color? buttonForeground,
    Color? secondaryButtonBackground,
    Color? secondaryButtonForeground,
    Color? muted,
    Color? mutedForeground,
    Color? border,
  }) => AppColorExtension(
    card:                      card                      ?? this.card,
    cardForeground:            cardForeground            ?? this.cardForeground,
    navbar:                    navbar                    ?? this.navbar,
    inputBackground:           inputBackground           ?? this.inputBackground,
    inputBorder:               inputBorder               ?? this.inputBorder,
    buttonBackground:          buttonBackground          ?? this.buttonBackground,
    buttonForeground:          buttonForeground          ?? this.buttonForeground,
    secondaryButtonBackground: secondaryButtonBackground ?? this.secondaryButtonBackground,
    secondaryButtonForeground: secondaryButtonForeground ?? this.secondaryButtonForeground,
    muted:                     muted                     ?? this.muted,
    mutedForeground:           mutedForeground           ?? this.mutedForeground,
    border:                    border                    ?? this.border,
  );

  @override
  AppColorExtension lerp(AppColorExtension? other, double t) {
    if (other == null) return this;
    return AppColorExtension(
      card:                      Color.lerp(card,                      other.card,                      t)!,
      cardForeground:            Color.lerp(cardForeground,            other.cardForeground,            t)!,
      navbar:                    Color.lerp(navbar,                    other.navbar,                    t)!,
      inputBackground:           Color.lerp(inputBackground,           other.inputBackground,           t)!,
      inputBorder:               Color.lerp(inputBorder,               other.inputBorder,               t)!,
      buttonBackground:          Color.lerp(buttonBackground,          other.buttonBackground,          t)!,
      buttonForeground:          Color.lerp(buttonForeground,          other.buttonForeground,          t)!,
      secondaryButtonBackground: Color.lerp(secondaryButtonBackground, other.secondaryButtonBackground, t)!,
      secondaryButtonForeground: Color.lerp(secondaryButtonForeground, other.secondaryButtonForeground, t)!,
      muted:                     Color.lerp(muted,                     other.muted,                     t)!,
      mutedForeground:           Color.lerp(mutedForeground,           other.mutedForeground,           t)!,
      border:                    Color.lerp(border,                    other.border,                    t)!,
    );
  }
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/theme/app_colors.dart".into(),
                content: r##"import 'package:flutter/material.dart';

abstract final class LightColors {
  //colorscheme==========================================
  static const primary             = Color(0xFF6366F1);
  static const onPrimary           = Color(0xFFFFFFFF);
  static const primaryContainer    = Color(0xFF6366F1);
  static const onPrimaryContainer  = Color(0xFFFFFFFF);
  static const secondary           = Color(0xFFF4E6FF);
  static const onSecondary         = Color(0xFF18181B);
  static const secondaryContainer  = Color(0xFFF4E6FF);
  static const onSecondaryContainer= Color(0xFF18181B);
  static const surface             = Color(0xFFFFFFFF);
  static const onSurface           = Color(0xFF09090B);
  static const error               = Color(0xFFEF4444);
  static const onError             = Color(0xFFFAFAFA);
  static const outline             = Color(0xFFFFFAFF);
  static const outlineVariant      = Color(0xFFFFFAFF);
  static const surfaceTint         = Color(0xFF6366F1);


  //extension================================================
  static const card                      = Color(0xFFFFFFFF);
  static const cardForeground            = Color(0xFF09090B);
  static const navbar                    = Color(0xFFF9EBFF);
  static const inputBackground           = Color(0xFFFFFFFF);
  static const inputBorder               = Color(0xFFFFFAFF);
  static const buttonBackground          = Color(0xFF6366F1);
  static const buttonForeground          = Color(0xFFFFFFFF);
  static const secondaryButtonBackground = Color(0xFFF4E6FF);
  static const secondaryButtonForeground = Color(0xFF18181B);
  static const muted                     = Color(0xFFF9EBFF);
  static const mutedForeground           = Color(0xFF5B5B5C);
  static const border                    = Color(0xFFFFFAFF);
}

abstract final class DarkColors {
  //colorscheme=============================================
  static const primary             = Color(0xFF857FFD);
  static const onPrimary           = Color(0xFFFFFFFF);
  static const primaryContainer    = Color(0xFF857FFD);
  static const onPrimaryContainer  = Color(0xFFFFFFFF);
  static const secondary           = Color(0xFF000846);
  static const onSecondary         = Color(0xFFFAFAFA);
  static const secondaryContainer  = Color(0xFF000846);
  static const onSecondaryContainer= Color(0xFFFAFAFA);
  static const surface             = Color(0xFF09090B);
  static const onSurface           = Color(0xFFFAFAFA);
  static const error               = Color(0xFF7F1D1D);
  static const onError             = Color(0xFFFAFAFA);
  static const outline             = Color(0xFF00000A);
  static const outlineVariant      = Color(0xFF00000A);
  static const surfaceTint         = Color(0xFF857FFD);


  //extension=============================================
  static const card                      = Color(0xFF09090B);
  static const cardForeground            = Color(0xFFFAFAFA);
  static const navbar                    = Color(0xFF00003E);
  static const inputBackground           = Color(0xFF09090B);
  static const inputBorder               = Color(0xFF00000A);
  static const buttonBackground          = Color(0xFF857FFD);
  static const buttonForeground          = Color(0xFFFFFFFF);
  static const secondaryButtonBackground = Color(0xFF000846);
  static const secondaryButtonForeground = Color(0xFFFAFAFA);
  static const muted                     = Color(0xFF00003E);
  static const mutedForeground           = Color(0xFF969696);
  static const border                    = Color(0xFF00000A);
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/theme/app_text_style.dart".into(),
                content: r##"import 'package:flutter/material.dart';
import 'package:google_fonts/google_fonts.dart';

import 'app_colors.dart';

abstract final class AppTextStyles {

  static TextTheme get light => _build(LightColors.onSurface);
  static TextTheme get dark  => _build(DarkColors.onSurface);

  static TextTheme _build(Color textColor) => TextTheme(
    headlineLarge: GoogleFonts.workSans(
      fontSize: 32, fontWeight: FontWeight.w500, color: textColor,
    ),
    headlineMedium: GoogleFonts.workSans(
      fontSize: 24, fontWeight: FontWeight.w500, color: textColor,
    ),
    titleLarge: GoogleFonts.workSans(
      fontSize: 20, fontWeight: FontWeight.w500, color: textColor,
    ),
    bodyLarge: GoogleFonts.nunitoSans(
      fontSize: 16, fontWeight: FontWeight.w400, color: textColor,
    ),
    bodyMedium: GoogleFonts.nunitoSans(
      fontSize: 14, fontWeight: FontWeight.w400, color: textColor,
    ),
    labelLarge: GoogleFonts.nunitoSans(
      fontSize: 14, fontWeight: FontWeight.w500, color: textColor,
    ),
  );
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/theme/app_themes.dart".into(),
                content: r##"import 'package:flutter/material.dart';
import 'light_theme.dart';
import 'dark_theme.dart';

class AppTheme {
  static ThemeData get light => lightTheme;
  static ThemeData get dark => darkTheme;
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/theme/components/appbar_theme.dart".into(),
                content: r##"import 'package:flutter/material.dart';

class AppBarStyles {
  static AppBarTheme build({
    required Color backgroundColor,
    required Color foregroundColor,
  }) => AppBarTheme(
    backgroundColor: backgroundColor,
    foregroundColor: foregroundColor,
    elevation: 0,
    scrolledUnderElevation: 0,
    surfaceTintColor: Colors.transparent,
    centerTitle: false,
    titleTextStyle: TextStyle(
      color: foregroundColor,
      fontSize: 20,
      fontWeight: FontWeight.w600,
    ),
  );
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/theme/components/bottom_nav_theme.dart".into(),
                content: r##"import 'package:flutter/material.dart';

class BottomNavStyles {
  static BottomNavigationBarThemeData build({
    required Color backgroundColor,
    required Color selectedColor,
    required Color unselectedColor,
  }) => BottomNavigationBarThemeData(
    backgroundColor: backgroundColor,
    selectedItemColor: selectedColor,
    unselectedItemColor: unselectedColor,
    elevation: 0,
    type: BottomNavigationBarType.fixed,
  );
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/theme/components/button_theme.dart".into(),
                content: r##"import 'package:flutter/material.dart';

class ButtonStyles {
  static ElevatedButtonThemeData elevated({
      required Color backgroundColor,
      required Color foregroundColor,
    }) => ElevatedButtonThemeData(
      style: ElevatedButton.styleFrom(
        backgroundColor: backgroundColor,
        foregroundColor: foregroundColor,
        minimumSize: const Size.fromHeight(48), // Standard height
        elevation: 0,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
      ),
    );

    static OutlinedButtonThemeData outlined({
      required Color foregroundColor,
      required Color borderColor,
    }) => OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        foregroundColor: foregroundColor,
        side: BorderSide(color: borderColor),
        minimumSize: const Size.fromHeight(48),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
      ),
    );

  static TextButtonThemeData text({
    required Color foregroundColor,
  }) => TextButtonThemeData(
    style: TextButton.styleFrom(
      foregroundColor: foregroundColor,
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
    ),
  );
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/theme/components/card_theme.dart".into(),
                content: r##"import 'package:flutter/material.dart';

class CardStyles {
  static CardThemeData build({
    required Color backgroundColor,
    required Color borderColor,
  }) => CardThemeData(
    color: backgroundColor,
    elevation: 0,
    shape: RoundedRectangleBorder(
      borderRadius: BorderRadius.circular(12),
      side: BorderSide(color: borderColor),
    ),
    margin: EdgeInsets.zero,
  );
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/theme/components/component_themes.dart".into(),
                content: r##"import 'package:flutter/material.dart';
import 'appbar_theme.dart';
import 'button_theme.dart';
import 'input_theme.dart';
import 'card_theme.dart';
import 'bottom_nav_theme.dart';

class ComponentThemes {
  final AppBarTheme               appBar;
  final ElevatedButtonThemeData   elevatedButton;
  final OutlinedButtonThemeData   outlinedButton;
  final TextButtonThemeData       textButton;
  final InputDecorationTheme      input;
  final CardThemeData             card;
  final BottomNavigationBarThemeData bottomNav;

  const ComponentThemes({
    required this.appBar,
    required this.elevatedButton,
    required this.outlinedButton,
    required this.textButton,
    required this.input,
    required this.card,
    required this.bottomNav,
  });

  static ComponentThemes light(ColorScheme cs) => ComponentThemes(
    appBar:         AppBarStyles.build(
                      backgroundColor: cs.surface,
                      foregroundColor: cs.onSurface,
                    ),
    elevatedButton: ButtonStyles.elevated(
                      backgroundColor: cs.primary,
                      foregroundColor: cs.onPrimary,
                    ),
    outlinedButton: ButtonStyles.outlined(
                      foregroundColor: cs.primary,
                      borderColor:     cs.outline,
                    ),
    textButton:     ButtonStyles.text(foregroundColor: cs.primary),
    input:          InputStyles.build(
                      fillColor:          cs.surface,
                      borderColor:        cs.outline,
                      focusedBorderColor: cs.primary,
                      hintColor:          cs.onSurfaceVariant,
                    ),
    card:           CardStyles.build(
                      backgroundColor: cs.surface,
                      borderColor:     cs.outline,
                    ),
    bottomNav:      BottomNavStyles.build(
                      backgroundColor: cs.surface,
                      selectedColor:   cs.primary,
                      unselectedColor: cs.onSurfaceVariant,
                    ),
  );

  static ComponentThemes dark(ColorScheme cs) => ComponentThemes(
    appBar:         AppBarStyles.build(
                      backgroundColor: cs.surface,
                      foregroundColor: cs.onSurface,
                    ),
    elevatedButton: ButtonStyles.elevated(
                      backgroundColor: cs.primary,
                      foregroundColor: cs.onPrimary,
                    ),
    outlinedButton: ButtonStyles.outlined(
                      foregroundColor: cs.primary,
                      borderColor:     cs.outline,
                    ),
    textButton:     ButtonStyles.text(foregroundColor: cs.primary),
    input:          InputStyles.build(
                      fillColor:          cs.surface,
                      borderColor:        cs.outline,
                      focusedBorderColor: cs.primary,
                      hintColor:          cs.onSurfaceVariant,
                    ),
    card:           CardStyles.build(
                      backgroundColor: cs.surface,
                      borderColor:     cs.outline,
                    ),
    bottomNav:      BottomNavStyles.build(
                      backgroundColor: cs.surface,
                      selectedColor:   cs.primary,
                      unselectedColor: cs.onSurfaceVariant,
                    ),
  );

  static SnackBarThemeData snackBar(ColorScheme cs) => SnackBarThemeData(
    backgroundColor: cs.inverseSurface,
    contentTextStyle: TextStyle(color: cs.onInverseSurface),
    behavior: SnackBarBehavior.floating,
    shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
  );
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/theme/components/input_theme.dart".into(),
                content: r##"import 'package:flutter/material.dart';

class InputStyles {
  static InputDecorationTheme build({
    required Color fillColor,
    required Color borderColor,
    required Color focusedBorderColor,
    required Color hintColor,
  }) => InputDecorationTheme(
    filled: true,
    fillColor: fillColor,
    hintStyle: TextStyle(color: hintColor, fontSize: 14),
    contentPadding: const EdgeInsets.symmetric(horizontal: 16, vertical: 14),
    border: OutlineInputBorder(
      borderRadius: BorderRadius.circular(4),
      borderSide: BorderSide(color: borderColor),
    ),
    enabledBorder: OutlineInputBorder(
      borderRadius: BorderRadius.circular(4),
      borderSide: BorderSide(color: borderColor),
    ),
    focusedBorder: OutlineInputBorder(
      borderRadius: BorderRadius.circular(4),
      borderSide: BorderSide(color: focusedBorderColor, width: 1.5),
    ),
    errorBorder: OutlineInputBorder(
      borderRadius: BorderRadius.circular(4),
      borderSide: BorderSide(color: Colors.red.shade400),
    ),
  );
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/theme/dark_theme.dart".into(),
                content: r##"import 'package:flutter/material.dart';
import 'app_color_extension.dart';
import 'app_colors.dart';
import 'app_text_style.dart';
import 'components/component_themes.dart';


const _scheme = ColorScheme.dark(
  primary:              DarkColors.primary,
  onPrimary:            DarkColors.onPrimary,
  primaryContainer:     DarkColors.primaryContainer,
  onPrimaryContainer:   DarkColors.onPrimaryContainer,
  secondary:            DarkColors.secondary,
  onSecondary:          DarkColors.onSecondary,
  secondaryContainer:   DarkColors.secondaryContainer,
  onSecondaryContainer: DarkColors.onSecondaryContainer,
  surface:              DarkColors.surface,
  onSurface:            DarkColors.onSurface,
  error:                DarkColors.error,
  onError:              DarkColors.onError,
  outline:              DarkColors.outline,
  outlineVariant:       DarkColors.outlineVariant,
  surfaceTint:          DarkColors.surfaceTint,
);

final ThemeData darkTheme = ThemeData(
  useMaterial3:            true,
  colorScheme:             _scheme,
  scaffoldBackgroundColor: DarkColors.surface,
  textTheme:               AppTextStyles.dark,
  appBarTheme:             ComponentThemes.dark(_scheme).appBar,
  elevatedButtonTheme:     ComponentThemes.dark(_scheme).elevatedButton,
  outlinedButtonTheme:     ComponentThemes.dark(_scheme).outlinedButton,
  textButtonTheme:         ComponentThemes.dark(_scheme).textButton,
  inputDecorationTheme:    ComponentThemes.dark(_scheme).input,
  cardTheme:               ComponentThemes.dark(_scheme).card,
  bottomNavigationBarTheme: ComponentThemes.dark(_scheme).bottomNav,
  extensions:              const [AppColorExtension.dark],
);"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/theme/light_theme.dart".into(),
                content: r##"import 'package:flutter/material.dart';
import 'app_color_extension.dart';
import 'app_colors.dart';
import 'app_text_style.dart';
import 'components/component_themes.dart';


const _scheme = ColorScheme.light(
  primary:              LightColors.primary,
  onPrimary:            LightColors.onPrimary,
  primaryContainer:     LightColors.primaryContainer,
  onPrimaryContainer:   LightColors.onPrimaryContainer,
  secondary:            LightColors.secondary,
  onSecondary:          LightColors.onSecondary,
  secondaryContainer:   LightColors.secondaryContainer,
  onSecondaryContainer: LightColors.onSecondaryContainer,
  surface:              LightColors.surface,
  onSurface:            LightColors.onSurface,
  error:                LightColors.error,
  onError:              LightColors.onError,
  outline:              LightColors.outline,
  outlineVariant:       LightColors.outlineVariant,
  surfaceTint:          LightColors.surfaceTint,
);

final ThemeData lightTheme = ThemeData(
  useMaterial3:            true,
  colorScheme:             _scheme,
  scaffoldBackgroundColor: LightColors.surface,
  textTheme:               AppTextStyles.light,
  appBarTheme:             ComponentThemes.light(_scheme).appBar,
  elevatedButtonTheme:     ComponentThemes.light(_scheme).elevatedButton,
  outlinedButtonTheme:     ComponentThemes.light(_scheme).outlinedButton,
  textButtonTheme:         ComponentThemes.light(_scheme).textButton,
  inputDecorationTheme:    ComponentThemes.light(_scheme).input,
  cardTheme:               ComponentThemes.light(_scheme).card,
  bottomNavigationBarTheme: ComponentThemes.light(_scheme).bottomNav,
  extensions:              const [AppColorExtension.light],
);"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/core/theme/theme_provider.dart".into(),
                content: r##"import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

class ThemeNotifier extends Notifier<ThemeMode> {
  @override
  ThemeMode build() => ThemeMode.system;

  void setThemeMode(ThemeMode mode) => state = mode;
  void toggleTheme() => state = state == ThemeMode.light ? ThemeMode.dark : ThemeMode.light;
}

final themeProvider = NotifierProvider<ThemeNotifier, ThemeMode>(() => ThemeNotifier());
"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/features/home/presentation/pages/home_page.dart".into(),
                content: r##"import 'dart:math' as math;
import 'package:flutter/material.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import '../../../../core/extension/context_extension.dart';

class HomePage extends HookConsumerWidget {
  const HomePage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // Hook for local counter state
    final count = useState(0);

    // Hook for hero animations
    final animationController = useAnimationController(
      duration: const Duration(seconds: 10),
    )..repeat();

    return Scaffold(
      backgroundColor: context.cs.surface,
      body: SingleChildScrollView(
        child: Column(
          children: [
            const SizedBox(height: 80),
            // --- BRANDED HERO SECTION ---
            _buildBrandedHeroStack(context, animationController),
            const SizedBox(height: 40),
            _buildHeroText(context),
            const SizedBox(height: 28),
            _buildMainActionButton(context, count),
            const SizedBox(height: 80),

            // --- GRADIENT DIVIDER ---
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 40),
              child: Container(
                height: 1,
                decoration: BoxDecoration(
                  gradient: LinearGradient(
                    colors: [
                      Colors.transparent,
                      context.ac.border,
                      Colors.transparent,
                    ],
                  ),
                ),
              ),
            ),

            // --- FEATURES GRID ---
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 60, horizontal: 24),
              child: LayoutBuilder(builder: (context, constraints) {
                bool isDesktop = constraints.maxWidth > 800;
                return Wrap(
                  spacing: 24,
                  runSpacing: 24,
                  children: [
                    _FeatureCard(
                      width: isDesktop ? (constraints.maxWidth / 3) - 24 : constraints.maxWidth,
                      title: "Flutter Hooks",
                      desc: "Manage widget lifecycle and local state without the boilerplate of StatefulWidgets.",
                      icon: Icons.anchor_rounded,
                      iconColor: const Color(0xFF00B0FF),
                    ),
                    _FeatureCard(
                      width: isDesktop ? (constraints.maxWidth / 3) - 24 : constraints.maxWidth,
                      title: "Riverpod 2.0",
                      desc: "Compile-safe, provider-based state management that works everywhere.",
                      icon: Icons.water_drop_rounded,
                      iconColor: const Color(0xFF00D2FF),
                    ),
                    _FeatureCard(
                      width: isDesktop ? (constraints.maxWidth / 3) - 24 : constraints.maxWidth,
                      title: "Clean Architecture",
                      desc: "Separation of concerns using Dio, GoRouter, and custom theme extensions.",
                      icon: Icons.auto_awesome_mosaic_rounded,
                      iconColor: context.cs.primary,
                    ),
                  ],
                );
              }),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildBrandedHeroStack(BuildContext context, AnimationController controller) {
    return SizedBox(
      height: 220,
      width: 220,
      child: Stack(
        alignment: Alignment.center,
        clipBehavior: Clip.none,
        children: [
          AnimatedBuilder(
            animation: controller,
            builder: (context, child) {
              return Container(
                width: 140 + (math.sin(controller.value * 2 * math.pi) * 10),
                height: 140 + (math.sin(controller.value * 2 * math.pi) * 10),
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  boxShadow: [
                    BoxShadow(
                      color: context.cs.primary.withOpacity(0.25),
                      blurRadius: 70,
                      spreadRadius: 25,
                    ),
                  ],
                ),
              );
            },
          ),
          Container(
            padding: const EdgeInsets.all(28),
            decoration: BoxDecoration(
              color: context.ac.card,
              shape: BoxShape.circle,
              border: Border.all(color: context.ac.border, width: 1.5),
              boxShadow: [
                BoxShadow(
                  color: Colors.black.withOpacity(0.05),
                  blurRadius: 25,
                  offset: const Offset(0, 12),
                )
              ],
            ),
            child: Container(
              width: 70,
              height: 70,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                gradient: const LinearGradient(
                  colors: [Color(0xFF00F3FF), Color(0xFFAB00FF), Color(0xFFFF00CC)],
                  begin: Alignment.topLeft,
                  end: Alignment.bottomRight,
                ),
                boxShadow: [
                  BoxShadow(
                    color: const Color(0xFFAB00FF).withOpacity(0.4),
                    blurRadius: 15,
                    spreadRadius: 2,
                  ),
                ],
              ),
              child: const Icon(
                Icons.unarchive_rounded,
                color: Colors.white,
                size: 38,
              ),
            ),
          ),
          Positioned(
            top: -2,
            right: -2,
            child: RotationTransition(
              turns: controller,
              child: _buildFloatingIcon(
                context,
                const Icon(Icons.anchor_rounded, color: Color(0xFF00B0FF), size: 30),
              ),
            ),
          ),
          Positioned(
            bottom: -2,
            left: -2,
            child: _buildFloatingIcon(
              context,
              const Icon(Icons.water_drop_rounded, color: Color(0xFF00D2FF), size: 30),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildFloatingIcon(BuildContext context, Widget child) {
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: context.ac.card,
        shape: BoxShape.circle,
        border: Border.all(color: context.ac.border),
        boxShadow: const [BoxShadow(color: Colors.black12, blurRadius: 10)],
      ),
      child: child,
    );
  }

  Widget _buildHeroText(BuildContext context) {
    return Column(
      children: [
        Text(
          "Get started Onpkg\nFlutter + Hooks",
          textAlign: TextAlign.center,
          style: context.textTheme.headlineLarge
        ),
        const SizedBox(height: 20),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 40),
          child: Text(
            "The ultimate developer setup with Riverpod, Hooks, GoRouter, and Clean Architecture.",
            textAlign: TextAlign.center,
            style: context.textTheme.bodyLarge?.copyWith(
              color: context.ac.mutedForeground,
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildMainActionButton(BuildContext context, ValueNotifier<int> count) {
    return Column(
      children: [
        GestureDetector(
          onTap: () => count.value++,
          child: AnimatedContainer(
            duration: const Duration(milliseconds: 200),
            padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 14),
            decoration: BoxDecoration(
              color: context.ac.buttonBackground,
              borderRadius: BorderRadius.circular(12),
              boxShadow: [
                BoxShadow(
                  color: context.ac.buttonBackground.withOpacity(0.3),
                  blurRadius: 15,
                  offset: const Offset(0, 8),
                )
              ],
            ),
            child: Text(
              "Count is ${count.value}",
              style: context.textTheme.labelLarge?.copyWith(
                color: context.ac.buttonForeground,
                fontWeight: FontWeight.w700,
              ),
            ),
          ),
        ),
        const SizedBox(height: 16),
        Text(
          "Edit home_page.dart to test HMR",
          style: context.textTheme.bodyMedium?.copyWith(
            color: context.ac.mutedForeground,
            fontFamily: 'monospace',
            fontSize: 12,
          ),
        ),
      ],
    );
  }
}

class _FeatureCard extends StatelessWidget {
  final String title;
  final String desc;
  final IconData icon;
  final Color iconColor;
  final double width;

  const _FeatureCard({
    required this.title,
    required this.desc,
    required this.icon,
    required this.iconColor,
    required this.width,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      width: width,
      padding: const EdgeInsets.all(32),
      decoration: BoxDecoration(
        color: context.ac.card,
        borderRadius: BorderRadius.circular(28),
        border: Border.all(color: context.ac.border),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Container(
            padding: const EdgeInsets.all(12),
            decoration: BoxDecoration(
              color: iconColor.withOpacity(0.1),
              borderRadius: BorderRadius.circular(12),
            ),
            child: Icon(icon, color: iconColor, size: 28),
          ),
          const SizedBox(height: 24),
          Text(
            title,
            style: context.textTheme.titleLarge?.copyWith(
              fontWeight: FontWeight.w600, // Work Sans
            ),
          ),
          const SizedBox(height: 12),
          Text(
            desc,
            style: context.textTheme.bodyMedium?.copyWith(
              color: context.ac.mutedForeground, // Nunito Sans
              height: 1.6,
            ),
          ),
        ],
      ),
    );
  }
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/onpkg_main.dart".into(),
                content: format!(
                    r##"import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'core/router/app_router.dart';
import 'core/theme/app_themes.dart';
import 'core/theme/theme_provider.dart';



void main() {{
  runApp(
    const ProviderScope(
      child: MyApp(),
    ),
  );
}}

class MyApp extends ConsumerWidget {{
  const MyApp({{super.key}});

  @override
  Widget build(BuildContext context, WidgetRef ref) {{
    final router = ref.watch(routerProvider);
    final themeMode = ref.watch(themeProvider);

    return MaterialApp.router(
      title: "{title}",
      debugShowCheckedModeBanner: false,
      themeMode: themeMode,
      darkTheme: AppTheme.dark,
      theme: AppTheme.light,
      routerConfig: router,
    );
  }}
}}"##,
                    title = title
                ).into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/shared/widgets/app_footer/app_footer.dart".into(),
                content: r##"import 'package:flutter/material.dart';
import '../../../../core/extension/context_extension.dart';

class AppFooter extends StatelessWidget {
  const AppFooter({super.key});

  @override
  Widget build(BuildContext context) {
    return Container(
      margin: const EdgeInsets.fromLTRB(24, 0, 24, 10), // Floats it off the bottom
      padding: const EdgeInsets.symmetric(vertical: 8, horizontal: 12),
      decoration: BoxDecoration(
        color: context.ac.card.withOpacity(0.8), // Glass-like effect
        borderRadius: BorderRadius.circular(24),
        border: Border.all(color: context.ac.border),
        boxShadow: [
          BoxShadow(
            color: Colors.black.withOpacity(0.05),
            blurRadius: 20,
            offset: const Offset(0, 10),
          ),
        ],
      ),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(24),
        child: Row(
          mainAxisAlignment: MainAxisAlignment.spaceAround,
          children: [
            _FooterItem(
              icon: Icons.home_rounded,
              label: "Home",
              isActive: true,
              onTap: () {},
            ),
            _FooterItem(
              icon: Icons.search_rounded,
              label: "Search",
              onTap: () {},
            ),
            _FooterItem(
              icon: Icons.person_outline_rounded,
              label: "Profile",
              onTap: () {},
            ),
          ],
        ),
      ),
    );
  }
}

class _FooterItem extends StatelessWidget {
  final IconData icon;
  final String label;
  final bool isActive;
  final VoidCallback onTap;

  const _FooterItem({
    required this.icon,
    required this.label,
    required this.onTap,
    this.isActive = false,
  });

  @override
  Widget build(BuildContext context) {
    final color = isActive ? context.cs.primary : context.ac.mutedForeground;

    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(16),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, color: color, size: 26),
            const SizedBox(height: 4),
            Text(
              label,
              style: context.textTheme.labelSmall?.copyWith(
                color: color,
                fontWeight: isActive ? FontWeight.bold : FontWeight.normal,
              ),
            ),
          ],
        ),
      ),
    );
  }
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/shared/widgets/app_top_navbar/app_top_navbar.dart".into(),
                content: r##"import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../../../core/extension/context_extension.dart';
import 'toggle_theme_button.dart';

class AppTopNavbar extends ConsumerWidget implements PreferredSizeWidget {
  const AppTopNavbar({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 16),
      decoration: BoxDecoration(
        color: context.cs.surface.withOpacity(0.8), // Glassmorphism
        border: Border(
          bottom: BorderSide(
            color: context.ac.border.withOpacity(0.5),
            width: 1,
          ),
        ),
      ),
      child: SafeArea(
        child: SizedBox(
          height: kToolbarHeight,
          child: Row(
            children: [
              // Logo/Brand Section
              Container(
                padding: const EdgeInsets.all(8),
                decoration: BoxDecoration(
                  color: context.cs.primary.withOpacity(0.1),
                  borderRadius: BorderRadius.circular(12),
                ),
                child: Icon(
                  Icons.auto_awesome_rounded,
                  color: context.cs.primary,
                  size: 20,
                ),
              ),
              const SizedBox(width: 12),
              Text(
                'Onpkg',
                style: context.textTheme.titleLarge
              ),
              const Spacer(),
              // Actions
              const ToggleThemeButton(size: 18),
              const SizedBox(width: 8),
              _buildModernNavButton(context, Icons.menu_rounded),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildModernNavButton(BuildContext context, IconData icon) {
    return IconButton(
      onPressed: () {},
      icon: Icon(icon, color: context.ac.mutedForeground),
      splashRadius: 24,
    );
  }

  @override
  Size get preferredSize => const Size.fromHeight(kToolbarHeight);
}"##.into(),
                binary_content: None,
            },
            StackFile {
                path: "lib/shared/widgets/app_top_navbar/toggle_theme_button.dart".into(),
                content: r##"import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../../../core/extension/context_extension.dart';
import '../../../core/theme/theme_provider.dart';


class ToggleThemeButton extends ConsumerWidget {
  const ToggleThemeButton({
    super.key,
    this.size = 20.0,
    this.padding = const EdgeInsets.all(5.0),
    this.color,
  });

  final double size;
  final EdgeInsetsGeometry padding;
  final Color? color;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final themeMode = ref.watch(themeProvider);

    return InkWell(
      onTap: () => ref.read(themeProvider.notifier).toggleTheme(),
      borderRadius: BorderRadius.circular(30),
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 300),
        padding: padding,
        decoration: BoxDecoration(
          color: context.ac.card,
          borderRadius: BorderRadius.circular(30),
          border: Border.all(
            color: context.ac.mutedForeground,
            width: 0.2,
          ),
        ),
        child: Icon(
          themeMode == ThemeMode.light ? Icons.dark_mode_outlined : Icons.light_mode_outlined,
          size: size,
          color: color ?? context.cs.onSurface,
        ),
      ),
    );
  }
}"##.into(),
                binary_content: None,
            },
        ],
    }
}
