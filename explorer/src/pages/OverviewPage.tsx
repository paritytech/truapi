import { useNavigate } from "react-router-dom";
import type { ReactNode } from "react";
import {
  ArrowLeftRight,
  ArrowRight,
  Box,
  FileText,
  HardDrive,
  Image,
  Key,
  Layers,
  Link,
  MessageSquare,
  Monitor,
  PenTool,
  Radio,
  RotateCcw,
  Shield,
  User,
  Wallet,
  Zap,
} from "lucide-react";
import { useVersion } from "../contexts/VersionContext";

const groupIcons: Record<string, ReactNode> = {
  "truapi-calls": <Zap size={20} />,
  permissions: <Shield size={20} />,
  "local-storage": <HardDrive size={20} />,
  "account-management": <User size={20} />,
  signing: <PenTool size={20} />,
  chat: <MessageSquare size={20} />,
  "statement-store": <FileText size={20} />,
  preimage: <Image size={20} />,
  "chain-interaction": <Link size={20} />,
  payment: <Wallet size={20} />,
  "entropy-derivation": <Key size={20} />,
};

const C = ({ children }: { children: ReactNode }) => (
  <code className="text-[13px] bg-slate-700/50 px-1 py-0.5 rounded font-mono">
    {children}
  </code>
);

export default function OverviewPage() {
  const navigate = useNavigate();
  const { groups, methods, version, versionPrefix } = useVersion();
  const totalMethods = methods.length;
  const reqResMethods = methods.filter(
    (m) => m.pattern === "request-response",
  ).length;
  const subMethods = methods.filter((m) => m.pattern === "subscription").length;
  const revSubMethods = methods.filter(
    (m) => m.pattern === "reverse-subscription",
  ).length;

  return (
    <div className="max-w-5xl mx-auto">
      <div className="mb-10 lg:mb-16 animate-slide-up">
        <div className="flex items-start gap-4 lg:gap-5 mb-6">
          <div className="w-12 h-12 lg:w-16 lg:h-16 rounded-xl lg:rounded-2xl bg-pink-600 flex items-center justify-center shrink-0 shadow-[0_0_40px_rgba(219,39,119,0.2)]">
            <span className="text-white text-lg lg:text-2xl font-bold font-display">
              T
            </span>
          </div>
          <div>
            <h1 className="text-2xl lg:text-4xl font-bold text-white font-display tracking-tight leading-tight">
              TrUAPI Protocol
            </h1>
            <div className="flex flex-wrap items-center gap-2 lg:gap-3 mt-2">
              <span className="text-sm text-slate-400">
                Protocol{" "}
                <span className="font-mono text-slate-300">
                  {version.label}
                </span>
              </span>
            </div>
          </div>
        </div>
        <p className="text-slate-300 text-lg leading-relaxed max-w-3xl">
          Complete reference for the protocol that mediates all communication
          between a<strong className="text-white"> host </strong> application
          and
          <strong className="text-white"> products </strong> running in
          sandboxes. This explorer is generated from the Rust source.
        </p>
      </div>

      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3 lg:gap-4 mb-10 lg:mb-16">
        <div className="stat-card stat-card-pink bg-slate-800/40 border border-slate-700/40 rounded-xl p-5 card-hover animate-slide-up stagger-1">
          <div className="text-3xl font-bold text-white font-display">
            {totalMethods}
          </div>
          <div className="text-sm text-slate-400 mt-1.5 font-medium">
            Total Methods
          </div>
        </div>
        <div className="stat-card stat-card-emerald bg-slate-800/40 border border-slate-700/40 rounded-xl p-5 card-hover animate-slide-up stagger-2">
          <div className="flex items-center gap-2.5">
            <ArrowLeftRight size={18} className="text-emerald-400" />
            <span className="text-3xl font-bold text-white font-display">
              {reqResMethods}
            </span>
          </div>
          <div className="text-sm text-slate-400 mt-1.5 font-medium">
            Request/Response
          </div>
        </div>
        <div className="stat-card stat-card-amber bg-slate-800/40 border border-slate-700/40 rounded-xl p-5 card-hover animate-slide-up stagger-3">
          <div className="flex items-center gap-2.5">
            <Radio size={18} className="text-amber-400" />
            <span className="text-3xl font-bold text-white font-display">
              {subMethods}
            </span>
          </div>
          <div className="text-sm text-slate-400 mt-1.5 font-medium">
            Subscriptions
          </div>
        </div>
        <div className="stat-card stat-card-purple bg-slate-800/40 border border-slate-700/40 rounded-xl p-5 card-hover animate-slide-up stagger-4">
          <div className="flex items-center gap-2.5">
            <RotateCcw size={18} className="text-purple-400" />
            <span className="text-3xl font-bold text-white font-display">
              {revSubMethods}
            </span>
          </div>
          <div className="text-sm text-slate-400 mt-1.5 font-medium">
            Reverse Sub
          </div>
        </div>
      </div>

      <div className="mb-10 lg:mb-16 animate-slide-up stagger-5">
        <h2 className="text-xl font-semibold text-white mb-5 font-display tracking-tight">
          Architecture
        </h2>
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4 mb-4">
          <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5 card-hover">
            <div className="flex items-center gap-2.5 mb-3">
              <div className="w-9 h-9 rounded-lg bg-purple-500/15 flex items-center justify-center">
                <Monitor size={17} className="text-purple-400" />
              </div>
              <h3 className="text-sm font-semibold text-white font-display">
                Host
              </h3>
            </div>
            <p className="text-sm text-slate-300 leading-relaxed">
              The host embeds products in sandboxes and provides accounts,
              signing, storage, chain access, chat, and permissions.
            </p>
          </div>
          <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5 card-hover">
            <div className="flex items-center gap-2.5 mb-3">
              <div className="w-9 h-9 rounded-lg bg-emerald-500/15 flex items-center justify-center">
                <Box size={17} className="text-emerald-400" />
              </div>
              <h3 className="text-sm font-semibold text-white font-display">
                Product
              </h3>
            </div>
            <p className="text-sm text-slate-300 leading-relaxed">
              A product runs inside a sandbox and requests services from the
              host through generated TrUAPI clients.
            </p>
          </div>
        </div>
        <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5 card-hover">
          <div className="flex items-center gap-2 mb-3">
            <Layers size={16} className="text-slate-400" />
            <h3 className="text-sm font-semibold text-white font-display">
              Communication Flow
            </h3>
          </div>
          <div className="bg-slate-900/60 rounded-lg p-4 font-mono text-xs sm:text-sm text-slate-400 leading-loose overflow-x-auto">
            <div className="space-y-0.5 min-w-[420px]">
              <div>
                <span className="text-emerald-400">request</span> -
                SCALE-encoded method call via <C>postMessage</C>
              </div>
              <div>
                <span className="text-purple-400">response</span> - result
                payload returned by the host
              </div>
              <div>
                <span className="text-amber-400">subscription</span> - start,
                receive, stop, and interrupt frames
              </div>
            </div>
          </div>
        </div>
      </div>

      <div className="animate-slide-up stagger-6">
        <h2 className="text-xl font-semibold text-white mb-5 font-display tracking-tight">
          Method Groups
        </h2>
        <div className="grid grid-cols-1 gap-3 mb-12">
          {groups.map((group, idx) => {
            const groupMethods = methods.filter((m) => m.groupId === group.id);
            return (
              <div
                key={group.id}
                className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5 card-hover cursor-pointer group animate-slide-up"
                style={{ animationDelay: `${0.4 + idx * 0.04}s` }}
                onClick={() =>
                  navigate(`${versionPrefix}/method/${groupMethods[0]?.id}`)
                }
              >
                <div className="flex items-start gap-4">
                  <div className="w-10 h-10 rounded-lg bg-slate-700/50 flex items-center justify-center text-slate-400 group-hover:text-pink-400 transition-colors shrink-0">
                    {groupIcons[group.id]}
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center justify-between mb-1">
                      <h3 className="font-semibold text-white text-sm font-display">
                        {group.name}
                      </h3>
                      <div className="flex items-center gap-1 text-slate-500 group-hover:text-pink-400 transition-colors">
                        <span className="text-sm">
                          {groupMethods.length} methods
                        </span>
                        <ArrowRight size={14} />
                      </div>
                    </div>
                    <p className="text-sm text-slate-400 leading-relaxed whitespace-pre-line">
                      {group.description.split("\n")[0]}
                    </p>
                    <div className="flex flex-wrap gap-1.5 mt-3">
                      {groupMethods.map((m) => (
                        <button
                          key={m.id}
                          onClick={(e) => {
                            e.stopPropagation();
                            navigate(`${versionPrefix}/method/${m.id}`);
                          }}
                          className="text-xs font-mono bg-slate-700/40 hover:bg-slate-700/70 text-slate-300 px-2 py-0.5 rounded transition-colors"
                          type="button"
                        >
                          {m.name}
                        </button>
                      ))}
                    </div>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
