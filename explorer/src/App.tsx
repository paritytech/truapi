import { useEffect, useState } from "react";
import { Navigate, Outlet, Route, Routes, useParams } from "react-router-dom";
import { Menu, Search } from "lucide-react";
import { findVersion, versions } from "./data/registry";
import Sidebar from "./components/Sidebar";
import SearchModal from "./components/SearchModal";
import ErrorBoundary from "./components/ErrorBoundary";
import OverviewPage from "./pages/OverviewPage";
import MethodPage from "./pages/MethodPage";
import TypesPage from "./pages/TypesPage";
import TypeDetailPage from "./pages/TypeDetailPage";

const SIDEBAR_WIDTH = 288;

function VersionLayout() {
  const { version: versionId } = useParams<{ version: string }>();
  const version = findVersion(versionId);
  const [searchOpen, setSearchOpen] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(false);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setSearchOpen((v) => !v);
      }
      if (e.key === "Escape") setSearchOpen(false);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  if (versionId !== version.id) {
    return <Navigate to={`/v/${version.id}/`} replace />;
  }

  return (
    <div className="flex min-h-screen bg-slate-925">
      <Sidebar
        open={sidebarOpen}
        onClose={() => setSidebarOpen(false)}
        width={SIDEBAR_WIDTH}
        versions={versions}
        current={version}
      />

      <main className="flex-1 min-w-0">
        <div className="sticky top-0 z-10 bg-slate-925/80 backdrop-blur-md border-b border-slate-700/40">
          <div className="flex items-center justify-between px-4 lg:px-8 py-3">
            <div className="flex items-center gap-3">
              <button
                onClick={() => setSidebarOpen(true)}
                className="lg:hidden p-1.5 text-slate-400 hover:text-white transition-colors"
              >
                <Menu size={20} />
              </button>
              <button
                onClick={() => setSearchOpen(true)}
                className="flex items-center gap-2 bg-slate-800/50 border border-slate-700/40 rounded-lg px-3 py-1.5 text-sm text-slate-400 hover:text-slate-200 hover:border-slate-600/50 transition-all duration-150"
              >
                <Search size={14} />
                <span>Search...</span>
                <kbd className="text-xs text-slate-400 bg-slate-700/50 px-1.5 py-0.5 rounded ml-4 font-mono hidden sm:inline">
                  {typeof navigator !== "undefined" &&
                  navigator.platform.includes("Mac")
                    ? "Cmd K"
                    : "Ctrl K"}
                </kbd>
              </button>
            </div>
          </div>
        </div>

        <div className="px-4 py-6 lg:px-8 lg:py-8">
          <ErrorBoundary>
            <Outlet context={{ version }} />
          </ErrorBoundary>
        </div>
      </main>

      <SearchModal
        open={searchOpen}
        onClose={() => setSearchOpen(false)}
        version={version}
      />
    </div>
  );
}

function NotFoundRedirect() {
  const fallback = versions[0]?.id ?? "main";
  return <Navigate to={`/v/${fallback}/`} replace />;
}

export default function App() {
  const fallback = versions[0]?.id ?? "main";
  return (
    <Routes>
      <Route path="/" element={<Navigate to={`/v/${fallback}/`} replace />} />
      <Route path="/v/:version" element={<VersionLayout />}>
        <Route index element={<OverviewPage />} />
        <Route
          path="method/:serviceName/:methodName"
          element={<MethodPage />}
        />
        <Route path="types" element={<TypesPage />} />
        <Route path="type/:typeId" element={<TypeDetailPage />} />
      </Route>
      <Route path="*" element={<NotFoundRedirect />} />
    </Routes>
  );
}
