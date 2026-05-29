import { Component, type ErrorInfo, type ReactNode } from "react";

type Props = {
  children: ReactNode;
  fallback?: (error: Error, reset: () => void) => ReactNode;
};

type State = { error: Error | null };

export default class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("ErrorBoundary caught error:", error, info);
  }

  reset = () => this.setState({ error: null });

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;

    if (this.props.fallback) return this.props.fallback(error, this.reset);

    return (
      <div className="p-6 max-w-2xl">
        <h1 className="text-xl font-semibold text-rose-300 mb-2">
          Something went wrong
        </h1>
        <p className="text-slate-400 text-sm mb-4">
          An error occurred while rendering this page.
        </p>
        <pre className="text-xs text-rose-200 bg-slate-900/60 border border-slate-700/40 rounded p-3 overflow-auto mb-4">
          {error.message}
        </pre>
        <button
          onClick={this.reset}
          className="text-sm text-sky-300 hover:text-sky-200 underline"
        >
          Try again
        </button>
      </div>
    );
  }
}
