import { useNavigate } from 'react-router-dom';
import { groups, methods } from '../data/types';
import {
  Zap,
  Shield,
  HardDrive,
  User,
  PenTool,
  MessageSquare,
  FileText,
  Image,
  Link,
  ArrowRight,
  ArrowLeftRight,
  Radio,
  RotateCcw,
  Monitor,
  Box,
  Layers,
} from 'lucide-react';

const groupIcons: Record<string, React.ReactNode> = {
  'host-calls': <Zap size={20} />,
  'permissions': <Shield size={20} />,
  'local-storage': <HardDrive size={20} />,
  'account-management': <User size={20} />,
  'signing': <PenTool size={20} />,
  'chat': <MessageSquare size={20} />,
  'statement-store': <FileText size={20} />,
  'preimage': <Image size={20} />,
  'chain-interaction': <Link size={20} />,
};

const C = ({ children, className = '' }: { children: React.ReactNode; className?: string }) => (
  <code className={`text-[13px] bg-slate-700/50 px-1 py-0.5 rounded font-mono ${className}`}>{children}</code>
);

export default function OverviewPage() {
  const navigate = useNavigate();

  const totalMethods = methods.length;
  const reqResMethods = methods.filter(m => m.pattern === 'request-response').length;
  const subMethods = methods.filter(m => m.pattern === 'subscription').length;
  const revSubMethods = methods.filter(m => m.pattern === 'reverse-subscription').length;

  return (
    <div className="max-w-5xl mx-auto">
      {/* Hero */}
      <div className="mb-10 lg:mb-16 animate-slide-up">
        <div className="flex items-start gap-4 lg:gap-5 mb-6">
          <div className="w-12 h-12 lg:w-16 lg:h-16 rounded-xl lg:rounded-2xl bg-pink-600 flex items-center justify-center shrink-0 shadow-[0_0_40px_rgba(219,39,119,0.2)]">
            <span className="text-white text-lg lg:text-2xl font-bold font-display">H</span>
          </div>
          <div>
            <h1 className="text-2xl lg:text-4xl font-bold text-white font-display tracking-tight leading-tight">
              TruAPI Protocol
            </h1>
            <div className="flex flex-wrap items-center gap-2 lg:gap-3 mt-2">
              <span className="text-sm text-slate-400">Protocol <span className="font-mono text-slate-300">v0.1</span></span>
              <span className="text-slate-700 hidden sm:inline">|</span>
              <span className="text-sm text-slate-400 font-mono">npm: @novasamatech/tru-api v0.6.6-1</span>
            </div>
          </div>
        </div>
        <p className="text-slate-300 text-lg leading-relaxed max-w-3xl">
          Complete reference for the protocol that mediates all communication between a
          <strong className="text-white"> host </strong> application and
          <strong className="text-white"> products </strong> running in sandboxes. All messages are SCALE-encoded binary
          sent via <C>postMessage</C>.
        </p>
      </div>

      {/* Stats */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3 lg:gap-4 mb-10 lg:mb-16">
        <div className="stat-card stat-card-pink bg-slate-800/40 border border-slate-700/40 rounded-xl p-5 card-hover animate-slide-up stagger-1">
          <div className="text-3xl font-bold text-white font-display">{totalMethods}</div>
          <div className="text-sm text-slate-400 mt-1.5 font-medium">Total Methods</div>
        </div>
        <div className="stat-card stat-card-emerald bg-slate-800/40 border border-slate-700/40 rounded-xl p-5 card-hover animate-slide-up stagger-2">
          <div className="flex items-center gap-2.5">
            <ArrowLeftRight size={18} className="text-emerald-400" />
            <span className="text-3xl font-bold text-white font-display">{reqResMethods}</span>
          </div>
          <div className="text-sm text-slate-400 mt-1.5 font-medium">Request/Response</div>
        </div>
        <div className="stat-card stat-card-amber bg-slate-800/40 border border-slate-700/40 rounded-xl p-5 card-hover animate-slide-up stagger-3">
          <div className="flex items-center gap-2.5">
            <Radio size={18} className="text-amber-400" />
            <span className="text-3xl font-bold text-white font-display">{subMethods}</span>
          </div>
          <div className="text-sm text-slate-400 mt-1.5 font-medium">Subscriptions</div>
        </div>
        <div className="stat-card stat-card-purple bg-slate-800/40 border border-slate-700/40 rounded-xl p-5 card-hover animate-slide-up stagger-4">
          <div className="flex items-center gap-2.5">
            <RotateCcw size={18} className="text-purple-400" />
            <span className="text-3xl font-bold text-white font-display">{revSubMethods}</span>
          </div>
          <div className="text-sm text-slate-400 mt-1.5 font-medium">Reverse Sub</div>
        </div>
      </div>

      {/* Architecture: Roles */}
      <div className="mb-10 lg:mb-16 animate-slide-up stagger-5">
        <h2 className="text-xl font-semibold text-white mb-5 font-display tracking-tight">Architecture</h2>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4 mb-4">
          {/* Host role */}
          <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5 card-hover">
            <div className="flex items-center gap-2.5 mb-3">
              <div className="w-9 h-9 rounded-lg bg-purple-500/15 flex items-center justify-center">
                <Monitor size={17} className="text-purple-400" />
              </div>
              <h3 className="text-sm font-semibold text-white font-display">Host</h3>
            </div>
            <p className="text-sm text-slate-300 leading-relaxed mb-3">
              The host is the parent application that embeds products in sandboxes. It acts as the
              <strong className="text-white"> gatekeeper and service provider</strong> &mdash; managing user accounts,
              signing transactions, mediating blockchain access, and enforcing permissions.
            </p>
            <div className="text-sm text-slate-400 space-y-1.5">
              <div className="flex items-start gap-2">
                <span className="text-purple-400 mt-0.5">&bull;</span>
                <span>Registers handlers via <C>container.handleMethodName(handler)</C></span>
              </div>
              <div className="flex items-start gap-2">
                <span className="text-purple-400 mt-0.5">&bull;</span>
                <span>Owns the user session, wallet keys, and chain connections</span>
              </div>
              <div className="flex items-start gap-2">
                <span className="text-purple-400 mt-0.5">&bull;</span>
                <span>Pushes data to products via subscriptions</span>
              </div>
            </div>
          </div>

          {/* Product role */}
          <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5 card-hover">
            <div className="flex items-center gap-2.5 mb-3">
              <div className="w-9 h-9 rounded-lg bg-emerald-500/15 flex items-center justify-center">
                <Box size={17} className="text-emerald-400" />
              </div>
              <h3 className="text-sm font-semibold text-white font-display">Product</h3>
            </div>
            <p className="text-sm text-slate-300 leading-relaxed mb-3">
              A product is an application running inside a sandbox. It uses the Host API to
              <strong className="text-white"> request services</strong> from the host &mdash; reading accounts,
              signing payloads, accessing chain data, and more.
            </p>
            <div className="text-sm text-slate-400 space-y-1.5">
              <div className="flex items-start gap-2">
                <span className="text-emerald-400 mt-0.5">&bull;</span>
                <span>Calls methods via <C>truApi.methodName(payload)</C></span>
              </div>
              <div className="flex items-start gap-2">
                <span className="text-emerald-400 mt-0.5">&bull;</span>
                <span>Receives product-derived accounts (isolated per product)</span>
              </div>
              <div className="flex items-start gap-2">
                <span className="text-emerald-400 mt-0.5">&bull;</span>
                <span>Subscribes to real-time data from the host</span>
              </div>
            </div>
          </div>
        </div>

        {/* Communication flow diagram */}
        <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5 card-hover">
          <div className="flex items-center gap-2 mb-3">
            <Layers size={16} className="text-slate-400" />
            <h3 className="text-sm font-semibold text-white font-display">Communication Flow</h3>
          </div>
          <div className="bg-slate-900/60 rounded-lg p-4 font-mono text-xs sm:text-sm text-slate-400 leading-loose overflow-x-auto">
            <div className="flex items-center justify-between gap-4 mb-1 min-w-[420px]">
              <span className="text-emerald-400 w-32 text-right">Product (sandbox)</span>
              <span className="text-slate-600 flex-1 text-center border-b border-dashed border-slate-700">&nbsp;</span>
              <span className="text-purple-400 w-32">Host (parent)</span>
            </div>
            <div className="space-y-0.5 pl-4 min-w-[420px]">
              <div className="flex items-center gap-2">
                <span className="text-slate-300 w-28 text-right shrink-0">handshake</span>
                <span className="text-emerald-400">&rarr;</span>
                <span className="text-slate-400">negotiate codec version (auto, every 50ms until ack)</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-slate-300 w-28 text-right shrink-0">request</span>
                <span className="text-emerald-400">&rarr;</span>
                <span className="text-slate-400">SCALE-encoded method call via postMessage</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-slate-300 w-28 text-right shrink-0">response</span>
                <span className="text-purple-400">&larr;</span>
                <span className="text-slate-400">Result&lt;Ok, Err&gt; back via postMessage</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-slate-300 w-28 text-right shrink-0">_start</span>
                <span className="text-emerald-400">&rarr;</span>
                <span className="text-slate-400">open subscription (multiplexed if same params)</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-slate-300 w-28 text-right shrink-0">_receive</span>
                <span className="text-purple-400">&larr;</span>
                <span className="text-slate-400">host pushes values (0..N times)</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-slate-300 w-28 text-right shrink-0">_stop / _interrupt</span>
                <span className="text-slate-500">&harr;</span>
                <span className="text-slate-400">either side can close</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* High-level Providers */}
      <div className="mb-10 lg:mb-16 animate-slide-up stagger-6">
        <h2 className="text-xl font-semibold text-white mb-5 font-display tracking-tight">SDK Providers</h2>
        <p className="text-sm text-slate-300 mb-4 leading-relaxed max-w-3xl">
          While products can call Host API methods directly, the SDK provides higher-level <strong className="text-white">providers</strong> that
          wrap groups of low-level protocol methods into ergonomic interfaces.
        </p>

        <div className="space-y-3">
          {/* PAPI Provider */}
          <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5 card-hover">
            <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-2 sm:gap-4 mb-2">
              <div>
                <h3 className="text-sm font-semibold text-white flex flex-wrap items-center gap-2 font-display">
                  <Link size={14} className="text-sky-400" />
                  PAPI Provider
                  <C>createPapiProvider(genesisHash)</C>
                </h3>
                <p className="text-xs text-slate-400 mt-0.5">from <C>@novasamatech/product-sdk</C></p>
              </div>
              <span className="text-xs bg-sky-500/10 text-sky-400 px-2 py-0.5 rounded-full border border-sky-500/20 whitespace-nowrap self-start">
                13 low-level methods
              </span>
            </div>
            <p className="text-sm text-slate-300 leading-relaxed mb-3">
              The <strong className="text-white">PAPI provider</strong> wraps the entire
              <strong className="text-white"> Chain Interaction</strong> group (Group 9) &mdash; all 13{' '}
              <C>remote_chain_*</C> methods &mdash; behind a standard <C>polkadot-api</C> {' '}
              <C>JsonRpcProvider</C> interface. This means products never need to call chain methods directly;
              they interact with Substrate chains through the familiar polkadot-api abstractions.
            </p>
            <div className="text-sm text-slate-400 leading-relaxed">
              On the host side, a single <C>container.handleChainConnection(factory)</C> call registers a {' '}
              <C>JsonRpcProvider</C> factory. The internal <C>chainConnectionManager</C> then handles all chain methods
              automatically &mdash; translating between the binary protocol and JSON-RPC, multiplexing follow
              subscriptions, tracking operations, and managing reference counts.
            </div>
          </div>

          {/* Product Chat Manager */}
          <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5 card-hover">
            <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-2 sm:gap-4 mb-2">
              <div>
                <h3 className="text-sm font-semibold text-white flex flex-wrap items-center gap-2 font-display">
                  <MessageSquare size={14} className="text-amber-400" />
                  Product Chat Manager
                  <C>createProductChatManager()</C>
                </h3>
                <p className="text-xs text-slate-400 mt-0.5">from <C>@novasamatech/product-sdk</C></p>
              </div>
              <span className="text-xs bg-amber-500/10 text-amber-400 px-2 py-0.5 rounded-full border border-amber-500/20 whitespace-nowrap self-start">
                reverse subscription
              </span>
            </div>
            <p className="text-sm text-slate-300 leading-relaxed">
              Manages the <strong className="text-slate-300">reverse-direction subscription</strong> for custom chat
              message rendering (<C>product_chat_custom_message_render_subscribe</C>). This is the only
              method where the host initiates and the product responds &mdash; providing a rendered
              <C>CustomRendererNode</C> UI tree for messages the host cannot render natively.
            </p>
          </div>
        </div>
      </div>

      {/* Protocol Basics */}
      <div className="mb-10 lg:mb-16 animate-slide-up stagger-7">
        <h2 className="text-xl font-semibold text-white mb-5 font-display tracking-tight">Communication Patterns</h2>
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5 card-hover">
            <div className="flex items-center gap-2 mb-3">
              <ArrowLeftRight size={16} className="text-emerald-400" />
              <h3 className="text-sm font-semibold text-white font-display">Request/Response</h3>
            </div>
            <p className="text-sm text-slate-300 leading-relaxed">
              The initiator sends a request; the responder returns exactly one response.
              On the product side, calls return a <C>ResultAsync</C>.
              All responses are wrapped in <C>{'Result<Ok, Err>'}</C>.
            </p>
          </div>
          <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5 card-hover">
            <div className="flex items-center gap-2 mb-3">
              <Radio size={16} className="text-amber-400" />
              <h3 className="text-sm font-semibold text-white font-display">Subscription</h3>
            </div>
            <p className="text-sm text-slate-300 leading-relaxed">
              A long-lived channel where the responder sends multiple values over time.
              Uses <C>_start</C>,{' '}
              <C>_receive</C>,{' '}
              <C>_stop</C>, and{' '}
              <C>_interrupt</C> messages. The transport multiplexes: duplicate subscriptions with the same params share a single wire subscription.
            </p>
          </div>
          <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5 card-hover">
            <div className="flex items-center gap-2 mb-3">
              <RotateCcw size={16} className="text-purple-400" />
              <h3 className="text-sm font-semibold text-white font-display">Reverse Subscription</h3>
            </div>
            <p className="text-sm text-slate-300 leading-relaxed">
              A special subscription where the <strong className="text-slate-300">host initiates</strong> and
              the <strong className="text-slate-300">product responds</strong>. Used only for custom chat message
              rendering, where the host asks the product to provide a UI tree.
            </p>
          </div>
          <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5 card-hover">
            <h3 className="text-sm font-semibold text-white mb-2 font-display">Handshake</h3>
            <p className="text-sm text-slate-300 leading-relaxed">
              Before any method calls, the transport automatically negotiates the codec version. The product sends{' '}
              <C>host_handshake</C> every 50ms until the host responds (10s timeout).
              Current codec version: <C>1</C> (JAM codec). Handled internally by the transport.
            </p>
          </div>
        </div>
      </div>

      {/* Groups */}
      <div className="animate-slide-up stagger-8">
        <h2 className="text-xl font-semibold text-white mb-5 font-display tracking-tight">Method Groups</h2>
        <div className="grid grid-cols-1 gap-3 mb-12">
          {groups.map((group, idx) => {
            const groupMethods = methods.filter(m => m.groupId === group.id);
            return (
              <div
                key={group.id}
                className={`bg-slate-800/30 border border-slate-700/40 rounded-xl p-5 card-hover cursor-pointer group animate-slide-up`}
                style={{ animationDelay: `${0.4 + idx * 0.04}s` }}
                onClick={() => navigate(`/method/${groupMethods[0]?.id}`)}
              >
                <div className="flex items-start gap-4">
                  <div className="w-10 h-10 rounded-lg bg-slate-700/50 flex items-center justify-center text-slate-400 group-hover:text-pink-400 transition-colors shrink-0">
                    {groupIcons[group.id]}
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center justify-between mb-1">
                      <h3 className="font-semibold text-white text-sm font-display">{group.name}</h3>
                      <div className="flex items-center gap-1 text-slate-500 group-hover:text-pink-400 transition-colors">
                        <span className="text-sm">{groupMethods.length} methods</span>
                        <ArrowRight size={14} />
                      </div>
                    </div>
                    <p className="text-sm text-slate-400 leading-relaxed">{group.description}</p>
                    <div className="flex flex-wrap gap-1.5 mt-3">
                      {groupMethods.map(m => (
                        <button
                          key={m.id}
                          onClick={(e) => { e.stopPropagation(); navigate(`/method/${m.id}`); }}
                          className="text-xs font-mono bg-slate-700/40 hover:bg-slate-700/70 text-slate-300 px-2 py-0.5 rounded transition-colors"
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
