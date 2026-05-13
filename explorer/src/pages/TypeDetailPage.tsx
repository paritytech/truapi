import { Fragment } from "react";
import { ArrowLeft, ArrowRight } from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";
import { TypeString } from "../components/TypeLink";
import { useVersion } from "../contexts/VersionContext";
import { versionedMethodRoutePath } from "../lib/routes";

export default function TypeDetailPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { getTypeById, methods, dataTypes, versionPrefix } = useVersion();
  const dt = getTypeById(id || "");

  if (!dt) {
    return (
      <div className="flex h-64 items-center justify-center">
        <p className="text-slate-400">Type not found</p>
      </div>
    );
  }

  const referencingMethods = methods.filter((method) => {
    const searchIn = [
      method.request,
      method.response,
      method.responseDescription || "",
      method.errorType || "",
    ].join(" ");
    return searchIn.includes(dt.name) || searchIn.includes(dt.id);
  });

  const relatedTypes = dataTypes.filter((typeDef) => {
    if (typeDef.id === dt.id) return false;
    return (
      typeDef.definition.includes(dt.id) ||
      typeDef.definition.includes(dt.name) ||
      dt.definition.includes(typeDef.id) ||
      dt.definition.includes(typeDef.name)
    );
  });

  return (
    <div className="mx-auto max-w-4xl">
      <div className="mb-6 flex items-center gap-2 text-sm text-slate-400 animate-fade-in">
        <button
          onClick={() => navigate(`${versionPrefix}/`)}
          className="transition-colors hover:text-white"
          type="button"
        >
          TrUAPI
        </button>
        <span>/</span>
        <button
          onClick={() => navigate(`${versionPrefix}/types`)}
          className="transition-colors hover:text-white"
          type="button"
        >
          Data Types
        </button>
        <span>/</span>
        <span className="text-white">{dt.name}</span>
      </div>

      <div className="mb-8 animate-slide-up">
        <div className="mb-2 flex items-center gap-3">
          <h1 className="font-mono text-2xl font-bold text-white">{dt.name}</h1>
          <span className="rounded-full border border-slate-600/30 bg-slate-700/50 px-2 py-0.5 text-xs text-slate-400">
            {dt.category}
          </span>
        </div>
        <p className="leading-relaxed text-slate-300">{dt.description}</p>
        {dt.source && (
          <p className="mt-2 font-mono text-sm text-slate-400">
            Source: {dt.source}
          </p>
        )}
      </div>

      <div className="mb-8 overflow-hidden rounded-xl border border-slate-700/40 bg-slate-800/30 card-hover animate-slide-up stagger-1">
        <div className="border-b border-slate-700/40 px-5 py-3">
          <h2 className="font-display text-sm font-semibold text-white">
            Definition
          </h2>
        </div>
        <div className="px-5 py-4">
          <div className="rounded-lg bg-slate-900/60 p-4 font-mono text-sm">
            <TypeString text={dt.definition} />
          </div>
        </div>
      </div>

      {dt.fields && dt.fields.length > 0 && (
        <div className="mb-8 overflow-hidden rounded-xl border border-slate-700/40 bg-slate-800/30 card-hover animate-slide-up stagger-2">
          <div className="border-b border-slate-700/40 px-5 py-3">
            <h2 className="font-display text-sm font-semibold text-white">
              Fields
            </h2>
          </div>
          <div className="grid sm:grid-cols-[auto_1fr] sm:gap-x-4">
            {dt.fields.map((field, index, fields) => {
              const isLast = index === fields.length - 1;
              return (
                <Fragment key={field.name}>
                  <code
                    className={`break-all px-5 pt-3 pb-1 font-mono text-sm text-emerald-400 sm:py-3 ${
                      !isLast ? "sm:border-b sm:border-slate-700/30" : ""
                    }`}
                  >
                    {field.name}
                  </code>
                  <div
                    className={`min-w-0 px-5 pb-3 sm:py-3 ${
                      !isLast ? "border-b border-slate-700/30" : ""
                    }`}
                  >
                    <TypeString text={field.type} />
                    {field.description && (
                      <p className="mt-0.5 text-sm text-slate-400">
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

      {dt.variants && dt.variants.length > 0 && (
        <div className="mb-8 overflow-hidden rounded-xl border border-slate-700/40 bg-slate-800/30 card-hover animate-slide-up stagger-3">
          <div className="border-b border-slate-700/40 px-5 py-3">
            <h2 className="font-display text-sm font-semibold text-white">
              Variants
            </h2>
          </div>
          <div className="grid sm:grid-cols-[auto_1fr] sm:gap-x-4">
            {dt.variants.map((variant, index, variants) => {
              const isLast = index === variants.length - 1;
              return (
                <Fragment key={variant.name}>
                  <code
                    className={`break-all px-5 pt-3 pb-1 font-mono text-sm text-amber-400 sm:py-3 ${
                      !isLast ? "sm:border-b sm:border-slate-700/30" : ""
                    }`}
                  >
                    {variant.name}
                  </code>
                  <div
                    className={`min-w-0 px-5 pb-3 sm:py-3 ${
                      !isLast ? "border-b border-slate-700/30" : ""
                    }`}
                  >
                    <TypeString text={variant.type} />
                    {variant.description && (
                      <p className="mt-0.5 text-sm text-slate-400">
                        {variant.description}
                      </p>
                    )}
                  </div>
                </Fragment>
              );
            })}
          </div>
        </div>
      )}

      {referencingMethods.length > 0 && (
        <div className="mb-8 overflow-hidden rounded-xl border border-slate-700/40 bg-slate-800/30 card-hover animate-slide-up stagger-4">
          <div className="border-b border-slate-700/40 px-5 py-3">
            <h2 className="font-display text-sm font-semibold text-white">
              Used by Methods
            </h2>
          </div>
          <div className="flex flex-wrap gap-2 px-5 py-3">
            {referencingMethods.map((method) => (
              <button
                key={method.id}
                onClick={() =>
                  navigate(versionedMethodRoutePath(versionPrefix, method))
                }
                className="inline-flex items-center gap-1.5 rounded-md border border-slate-600/30 bg-slate-700/30 px-3 py-1.5 font-mono text-xs text-slate-300 transition-all duration-150 hover:bg-slate-700/50 hover:text-white hover:shadow-[0_2px_8px_rgba(0,0,0,0.2)]"
                type="button"
              >
                <ArrowRight size={10} />
                {method.name}
              </button>
            ))}
          </div>
        </div>
      )}

      {relatedTypes.length > 0 && (
        <div className="mb-8 overflow-hidden rounded-xl border border-slate-700/40 bg-slate-800/30 card-hover animate-slide-up stagger-5">
          <div className="border-b border-slate-700/40 px-5 py-3">
            <h2 className="font-display text-sm font-semibold text-white">
              Related Types
            </h2>
          </div>
          <div className="flex flex-wrap gap-2 px-5 py-3">
            {relatedTypes.map((typeDef) => (
              <button
                key={typeDef.id}
                onClick={() => navigate(`${versionPrefix}/type/${typeDef.id}`)}
                className="inline-flex items-center gap-1.5 rounded-md border border-sky-500/20 bg-sky-500/10 px-3 py-1.5 font-mono text-xs text-sky-400 transition-all duration-150 hover:bg-sky-500/20"
                type="button"
              >
                {typeDef.name}
              </button>
            ))}
          </div>
        </div>
      )}

      <button
        onClick={() => navigate(`${versionPrefix}/types`)}
        className="group mt-4 flex items-center gap-2 text-sm text-slate-400 transition-colors hover:text-white animate-fade-in"
        type="button"
      >
        <ArrowLeft
          size={16}
          className="transition-transform group-hover:-translate-x-0.5"
        />
        Back to all types
      </button>
    </div>
  );
}
