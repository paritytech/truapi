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
  <code className={`text-xs bg-slate-700/50 px-1 py-0.5 rounded font-mono ${className}`}>{children}</code>
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
      <div className="mb-12">
        <div className="flex items-center gap-3 mb-4">
          <div className="w-12 h-12 rounded-xl bg-gradient-to-br from-pink-500 to-purple-600 flex items-center justify-center">
            <span className="text-white text-xl font-bold">H</span>
          </div>
          <div>
            <h1 className="text-3xl font-bold text-white">Host API Protocol</h1>
            <div className="flex items-center gap-3 mt-0.5">
              <span className="text-sm text-slate-400">Protocol <span className="font-mono text-slate-300">v0.1</span></span>
              <span className="text-slate-600">|</span>
              <span className="text-xs text-slate-500 font-mono">npm: @novasamatech/host-api v0.6.6-1</span>
            </div>
          </div>
        </div>
        <p className="text-slate-300 text-lg leading-relaxed max-w-3xl">
          Complete reference for the protocol that mediates all communication between a
          <strong className="text-white"> host </strong> application and
          <strong className="text-white"> products </strong> running inside iframes. All messages are SCALE-encoded binary
          sent via <C>postMessage</C>.
        </p>
      </div>

      {/* Stats */}
      <div className="grid grid-cols-4 gap-4 mb-12">
        <div className="bg-slate-800/50 border border-slate-700/50 rounded-xl p-4">
          <div className="text-2xl font-bold text-white">{totalMethods}</div>
          <div className="text-xs text-slate-400 mt-1">Total Methods</div>
        </div>
        <div className="bg-slate-800/50 border border-slate-700/50 rounded-xl p-4">
          <div className="flex items-center gap-2">
            <ArrowLeftRight size={16} className="text-emerald-400" />
            <span className="text-2xl font-bold text-white">{reqResMethods}</span>
          </div>
          <div className="text-xs text-slate-400 mt-1">Request/Response</div>
        </div>
        <div className="bg-slate-800/50 border border-slate-700/50 rounded-xl p-4">
          <div className="flex items-center gap-2">
            <Radio size={16} className="text-amber-400" />
            <span className="text-2xl font-bold text-white">{subMethods}</span>
          </div>
          <div className="text-xs text-slate-400 mt-1">Subscriptions</div>
        </div>
        <div className="bg-slate-800/50 border border-slate-700/50 rounded-xl p-4">
          <div className="flex items-center gap-2">
            <RotateCcw size={16} className="text-purple-400" />
            <span className="text-2xl font-bold text-white">{revSubMethods}</span>
          </div>
          <div className="text-xs text-slate-400 mt-1">Reverse Sub</div>
        </div>
      </div>

      {/* Architecture: Roles */}
      <div className="mb-12">
        <h2 className="text-xl font-semibold text-white mb-4">Architecture</h2>

        <div className="grid grid-cols-2 gap-4 mb-4">
          {/* Host role */}
          <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5">
            <div className="flex items-center gap-2.5 mb-3">
              <div className="w-8 h-8 rounded-lg bg-purple-500/15 flex items-center justify-center">
                <Monitor size={16} className="text-purple-400" />
              </div>
              <h3 className="text-sm font-semibold text-white">Host</h3>
            </div>
            <p className="text-sm text-slate-400 leading-relaxed mb-3">
              The host is the parent application that embeds products in iframes. It acts as the
              <strong className="text-slate-300"> gatekeeper and service provider</strong> &mdash; managing user accounts,
              signing transactions, mediating blockchain access, and enforcing permissions.
            </p>
            <div className="text-xs text-slate-500 space-y-1">
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
          <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5">
            <div className="flex items-center gap-2.5 mb-3">
              <div className="w-8 h-8 rounded-lg bg-emerald-500/15 flex items-center justify-center">
                <Box size={16} className="text-emerald-400" />
              </div>
              <h3 className="text-sm font-semibold text-white">Product</h3>
            </div>
            <p className="text-sm text-slate-400 leading-relaxed mb-3">
              A product is a sandboxed application running inside an iframe. It uses the Host API to
              <strong className="text-slate-300"> request services</strong> from the host &mdash; reading accounts,
              signing payloads, accessing chain data, and more.
            </p>
            <div className="text-xs text-slate-500 space-y-1">
              <div className="flex items-start gap-2">
                <span className="text-emerald-400 mt-0.5">&bull;</span>
                <span>Calls methods via <C>hostApi.methodName(payload)</C></span>
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
        <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5">
          <div className="flex items-center gap-2 mb-3">
            <Layers size={16} className="text-slate-400" />
            <h3 className="text-sm font-semibold text-white">Communication Flow</h3>
          </div>
          <div className="bg-slate-900/60 rounded-lg p-4 font-mono text-xs text-slate-400 leading-loose">
            <div className="flex items-center justify-between gap-4 mb-1">
              <span className="text-emerald-400 w-32 text-right">Product (iframe)</span>
              <span className="text-slate-600 flex-1 text-center border-b border-dashed border-slate-700">&nbsp;</span>
              <span className="text-purple-400 w-32">Host (parent)</span>
            </div>
            <div className="space-y-0.5 pl-4">
              <div className="flex items-center gap-2">
                <span className="text-slate-500 w-28 text-right">handshake</span>
                <span className="text-emerald-400">&rarr;</span>
                <span className="text-slate-500">negotiate codec version (auto, every 50ms until ack)</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-slate-500 w-28 text-right">request</span>
                <span className="text-emerald-400">&rarr;</span>
                <span className="text-slate-500">SCALE-encoded method call via postMessage</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-slate-500 w-28 text-right">response</span>
                <span className="text-purple-400">&larr;</span>
                <span className="text-slate-500">Result&lt;Ok, Err&gt; back via postMessage</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-slate-500 w-28 text-right">_start</span>
                <span className="text-emerald-400">&rarr;</span>
                <span className="text-slate-500">open subscription (multiplexed if same params)</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-slate-500 w-28 text-right">_receive</span>
                <span className="text-purple-400">&larr;</span>
                <span className="text-slate-500">host pushes values (0..N times)</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-slate-500 w-28 text-right">_stop / _interrupt</span>
                <span className="text-slate-600">&harr;</span>
                <span className="text-slate-500">either side can close</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* High-level Providers */}
      <div className="mb-12">
        <h2 className="text-xl font-semibold text-white mb-4">SDK Providers</h2>
        <p className="text-sm text-slate-400 mb-4 leading-relaxed max-w-3xl">
          While products can call Host API methods directly, the SDK provides higher-level <strong className="text-slate-300">providers</strong> that
          wrap groups of low-level protocol methods into ergonomic interfaces.
        </p>

        <div className="space-y-3">
          {/* PAPI Provider */}
          <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5">
            <div className="flex items-start justify-between gap-4 mb-2">
              <div>
                <h3 className="text-sm font-semibold text-white flex items-center gap-2">
                  <Link size={14} className="text-sky-400" />
                  PAPI Provider
                  <C>createPapiProvider(genesisHash)</C>
                </h3>
                <p className="text-xs text-slate-500 mt-0.5">from <C>@novasamatech/product-sdk</C></p>
              </div>
              <span className="text-[10px] bg-sky-500/10 text-sky-400 px-2 py-0.5 rounded-full border border-sky-500/20 whitespace-nowrap">
                13 low-level methods
              </span>
            </div>
            <p className="text-sm text-slate-400 leading-relaxed mb-3">
              The <strong className="text-slate-300">PAPI provider</strong> wraps the entire
              <strong className="text-slate-300"> Chain Interaction</strong> group (Group 9) &mdash; all 13{' '}
              <C>remote_chain_*</C> methods &mdash; behind a standard <C>polkadot-api</C> {' '}
              <C>JsonRpcProvider</C> interface. This means products never need to call chain methods directly;
              they interact with Substrate chains through the familiar polkadot-api abstractions.
            </p>
            <div className="text-xs text-slate-500 leading-relaxed">
              On the host side, a single <C>container.handleChainConnection(factory)</C> call registers a {' '}
              <C>JsonRpcProvider</C> factory. The internal <C>chainConnectionManager</C> then handles all chain methods
              automatically &mdash; translating between the binary protocol and JSON-RPC, multiplexing follow
              subscriptions, tracking operations, and managing reference counts.
            </div>
          </div>

          {/* Product Chat Manager */}
          <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5">
            <div className="flex items-start justify-between gap-4 mb-2">
              <div>
                <h3 className="text-sm font-semibold text-white flex items-center gap-2">
                  <MessageSquare size={14} className="text-amber-400" />
                  Product Chat Manager
                  <C>createProductChatManager()</C>
                </h3>
                <p className="text-xs text-slate-500 mt-0.5">from <C>@novasamatech/product-sdk</C></p>
              </div>
              <span className="text-[10px] bg-amber-500/10 text-amber-400 px-2 py-0.5 rounded-full border border-amber-500/20 whitespace-nowrap">
                reverse subscription
              </span>
            </div>
            <p className="text-sm text-slate-400 leading-relaxed">
              Manages the <strong className="text-slate-300">reverse-direction subscription</strong> for custom chat
              message rendering (<C>product_chat_custom_message_render_subscribe</C>). This is the only
              method where the host initiates and the product responds &mdash; providing a rendered
              <C>CustomRendererNode</C> UI tree for messages the host cannot render natively.
            </p>
          </div>
        </div>
      </div>

      {/* Protocol Basics */}
      <div className="mb-12">
        <h2 className="text-xl font-semibold text-white mb-4">Communication Patterns</h2>
        <div className="grid grid-cols-2 gap-4">
          <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5">
            <div className="flex items-center gap-2 mb-3">
              <ArrowLeftRight size={16} className="text-emerald-400" />
              <h3 className="text-sm font-semibold text-white">Request/Response</h3>
            </div>
            <p className="text-sm text-slate-400 leading-relaxed">
              The initiator sends a request; the responder returns exactly one response.
              On the product side, calls return a <C>ResultAsync</C>.
              All responses are wrapped in <C>{'Result<Ok, Err>'}</C>.
            </p>
          </div>
          <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5">
            <div className="flex items-center gap-2 mb-3">
              <Radio size={16} className="text-amber-400" />
              <h3 className="text-sm font-semibold text-white">Subscription</h3>
            </div>
            <p className="text-sm text-slate-400 leading-relaxed">
              A long-lived channel where the responder sends multiple values over time.
              Uses <C>_start</C>,{' '}
              <C>_receive</C>,{' '}
              <C>_stop</C>, and{' '}
              <C>_interrupt</C> messages. The transport multiplexes: duplicate subscriptions with the same params share a single wire subscription.
            </p>
          </div>
          <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5">
            <div className="flex items-center gap-2 mb-3">
              <RotateCcw size={16} className="text-purple-400" />
              <h3 className="text-sm font-semibold text-white">Reverse Subscription</h3>
            </div>
            <p className="text-sm text-slate-400 leading-relaxed">
              A special subscription where the <strong className="text-slate-300">host initiates</strong> and
              the <strong className="text-slate-300">product responds</strong>. Used only for custom chat message
              rendering, where the host asks the product to provide a UI tree.
            </p>
          </div>
          <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5">
            <h3 className="text-sm font-semibold text-white mb-2">Handshake</h3>
            <p className="text-sm text-slate-400 leading-relaxed">
              Before any method calls, the transport automatically negotiates the codec version. The product sends{' '}
              <C>host_handshake</C> every 50ms until the host responds (10s timeout).
              Current codec version: <C>1</C> (JAM codec). Handled internally by the transport.
            </p>
          </div>
        </div>
      </div>

      {/* Groups */}
      <h2 className="text-xl font-semibold text-white mb-4">Method Groups</h2>
      <div className="grid grid-cols-1 gap-3 mb-12">
        {groups.map(group => {
          const groupMethods = methods.filter(m => m.groupId === group.id);
          return (
            <div
              key={group.id}
              className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5 hover:border-slate-600/50 transition-colors cursor-pointer group"
              onClick={() => navigate(`/method/${groupMethods[0]?.id}`)}
            >
              <div className="flex items-start gap-4">
                <div className="w-10 h-10 rounded-lg bg-slate-700/50 flex items-center justify-center text-slate-400 group-hover:text-pink-400 transition-colors shrink-0">
                  {groupIcons[group.id]}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center justify-between mb-1">
                    <h3 className="font-semibold text-white text-sm">{group.name}</h3>
                    <div className="flex items-center gap-1 text-slate-500 group-hover:text-pink-400 transition-colors">
                      <span className="text-xs">{groupMethods.length} methods</span>
                      <ArrowRight size={14} />
                    </div>
                  </div>
                  <p className="text-xs text-slate-400 leading-relaxed">{group.description}</p>
                  <div className="flex flex-wrap gap-1.5 mt-3">
                    {groupMethods.map(m => (
                      <button
                        key={m.id}
                        onClick={(e) => { e.stopPropagation(); navigate(`/method/${m.id}`); }}
                        className="text-[10px] font-mono bg-slate-700/40 hover:bg-slate-700/70 text-slate-300 px-2 py-0.5 rounded transition-colors"
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
  );
}
