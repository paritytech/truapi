/// Typed success/failure result, the Dart analogue of the TypeScript client's
/// `neverthrow` `Result`.
///
/// Requests return `Future<Result<Ok, Err>>`: protocol-level outcomes (the
/// host answered, with either a success value or a typed domain/framework
/// error) are values, not exceptions. Thrown errors are reserved for transport
/// and decode faults.
library;

/// A success ([Ok]) or failure ([Err]) outcome carrying typed payloads.
sealed class Result<T, E> {
  const Result();

  /// Whether this is an [Ok].
  bool get isOk => this is Ok<T, E>;

  /// Whether this is an [Err].
  bool get isErr => this is Err<T, E>;

  /// The success value, or `null` for an [Err].
  T? get okOrNull => switch (this) {
        Ok<T, E>(value: final v) => v,
        Err<T, E>() => null,
      };

  /// The error value, or `null` for an [Ok].
  E? get errOrNull => switch (this) {
        Ok<T, E>() => null,
        Err<T, E>(error: final e) => e,
      };

  /// Fold both branches into a single value.
  R match<R>({
    required R Function(T value) ok,
    required R Function(E error) err,
  }) =>
      switch (this) {
        Ok<T, E>(value: final v) => ok(v),
        Err<T, E>(error: final e) => err(e),
      };
}

/// Successful [Result] carrying a value of type `T`.
final class Ok<T, E> extends Result<T, E> {
  const Ok(this.value);

  /// The success payload.
  final T value;

  @override
  bool operator ==(Object other) => other is Ok<T, E> && other.value == value;

  @override
  int get hashCode => Object.hash(Ok, value);

  @override
  String toString() => 'Ok($value)';
}

/// Failed [Result] carrying an error of type `E`.
final class Err<T, E> extends Result<T, E> {
  const Err(this.error);

  /// The failure payload.
  final E error;

  @override
  bool operator ==(Object other) => other is Err<T, E> && other.error == error;

  @override
  int get hashCode => Object.hash(Err, error);

  @override
  String toString() => 'Err($error)';
}
