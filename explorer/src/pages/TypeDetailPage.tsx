import { Fragment } from "react";
import { Link, useOutletContext, useParams } from "react-router-dom";
import { ArrowLeft } from "lucide-react";
import type { VersionEntry } from "../data/types";
import { findType, usedByType } from "../data/registry";
import CodeBlock from "../components/CodeBlock";
import { TypeString } from "../components/TypeLink";

/** Detail page for a single type. */
export default function TypeDetailPage() {
  const { version } = useOutletContext<{ version: VersionEntry }>();
  const { typeId } = useParams<{ typeId: string }>();
  const type = typeId ? findType(version, typeId) : null;

  if (!type) {
    return (
      <div className="flex items-center justify-center h-64">
        <p className="text-slate-400">Type not found</p>
      </div>
    );
  }

  const prefix = `/v/${version.id}`;
  const referencing = usedByType(version, type.id);

  return (
    <div className="max-w-4xl mx-auto">
      <div className="flex items-center gap-2 text-sm text-slate-400 mb-6 animate-fade-in">
        <Link to={`${prefix}/`} className="hover:text-white transition-colors">
          TrUAPI
        </Link>
        <span>/</span>
        <Link
          to={`${prefix}/types`}
          className="hover:text-white transition-colors"
        >
          Data Types
        </Link>
        <span>/</span>
        <span className="text-white font-mono">{type.name}</span>
      </div>

      <div className="mb-8 animate-slide-up">
        <div className="flex items-center gap-3 mb-2">
          <h1 className="text-2xl font-bold text-white font-mono">
            {type.name}
          </h1>
          <span className="text-xs px-2 py-0.5 rounded-full bg-slate-700/50 text-slate-400 border border-slate-600/30">
            {type.category}
          </span>
        </div>
        {type.description && (
          <p className="text-slate-300 leading-relaxed">{type.description}</p>
        )}
      </div>

      <div className="mb-8 animate-slide-up">
        <h2 className="text-sm font-semibold text-white font-display mb-3">
          Definition
        </h2>
        <CodeBlock code={type.definition} />
      </div>

      {type.fields && type.fields.length > 0 && (
        <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl overflow-hidden mb-8 card-hover animate-slide-up">
          <div className="border-b border-slate-700/40 px-5 py-3">
            <h2 className="text-sm font-semibold text-white font-display">
              Fields
            </h2>
          </div>
          <div className="grid sm:grid-cols-[auto_1fr] sm:gap-x-4">
            {type.fields.map((field, i, arr) => {
              const last = i === arr.length - 1;
              return (
                <Fragment key={i}>
                  <code
                    className={`text-sm font-mono text-emerald-400 break-all px-5 pt-3 pb-1 sm:py-3 ${
                      !last ? "sm:border-b sm:border-slate-700/30" : ""
                    }`}
                  >
                    {field.name}
                  </code>
                  <div
                    className={`px-5 pb-3 sm:py-3 min-w-0 ${
                      !last ? "border-b border-slate-700/30" : ""
                    }`}
                  >
                    <TypeString
                      text={field.type}
                      versionId={version.id}
                      types={version.types}
                    />
                    {field.description && (
                      <p className="text-sm text-slate-400 mt-0.5">
                        {field.description}
                      </p>
                    )}
                  </div>
                </Fragment>
              );
            })}
          </div>
        </div>
      )}

      {type.variants && type.variants.length > 0 && (
        <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl overflow-hidden mb-8 card-hover animate-slide-up">
          <div className="border-b border-slate-700/40 px-5 py-3">
            <h2 className="text-sm font-semibold text-white font-display">
              Variants
            </h2>
          </div>
          <div className="grid sm:grid-cols-[auto_1fr] sm:gap-x-4">
            {type.variants.map((v, i, arr) => {
              const last = i === arr.length - 1;
              return (
                <Fragment key={i}>
                  <code
                    className={`text-sm font-mono text-amber-400 break-all px-5 pt-3 pb-1 sm:py-3 ${
                      !last ? "sm:border-b sm:border-slate-700/30" : ""
                    }`}
                  >
                    {v.name}
                  </code>
                  <div
                    className={`px-5 pb-3 sm:py-3 min-w-0 ${
                      !last ? "border-b border-slate-700/30" : ""
                    }`}
                  >
                    <TypeString
                      text={v.type}
                      versionId={version.id}
                      types={version.types}
                    />
                    {v.description && (
                      <p className="text-sm text-slate-400 mt-0.5">
                        {v.description}
                      </p>
                    )}
                  </div>
                </Fragment>
              );
            })}
          </div>
        </div>
      )}

      {referencing.length > 0 && (
        <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl overflow-hidden mb-8 card-hover animate-slide-up">
          <div className="border-b border-slate-700/40 px-5 py-3">
            <h2 className="text-sm font-semibold text-white font-display">
              Used by Methods
            </h2>
          </div>
          <div className="px-5 py-3 flex flex-wrap gap-2">
            {referencing.map((m) => (
              <Link
                key={m.name}
                to={`${prefix}/method/${m.name}`}
                className="px-3 py-1.5 rounded-md text-xs font-mono bg-slate-700/30 text-slate-300 hover:bg-slate-700/50 hover:text-white border border-slate-600/30 transition-all duration-150"
              >
                {m.name}
              </Link>
            ))}
          </div>
        </div>
      )}

      <Link
        to={`${prefix}/types`}
        className="inline-flex items-center gap-2 text-sm text-slate-400 hover:text-white transition-colors mt-4 group animate-fade-in"
      >
        <ArrowLeft
          size={16}
          className="group-hover:-translate-x-0.5 transition-transform"
        />
        Back to all types
      </Link>
    </div>
  );
}
