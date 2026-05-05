'use client';

import { Suspense, useEffect, useState } from 'react';
import { useSearchParams } from 'next/navigation';
import Link from 'next/link';

function PageContent() {
  const searchParams = useSearchParams();
  const [fragment, setFragment] = useState('');
  const [fullUrl, setFullUrl] = useState('');

  useEffect(() => {
    setFullUrl(window.location.href);
    setFragment(window.location.hash);
  }, []);

  const params = Array.from(searchParams.entries());
  const fragmentParts = fragment.replace('#', '').split('&').filter(Boolean);

  return (
    <div className="navtest">
      <div className="navtest__intro">
        <div className="view__breadcrumb">Diagnostics</div>
        <h1 className="view__title">
          <span className="view__slash">/</span>
          <span className="view__method">navigation_test</span>
        </h1>
        <p className="panel__desc">
          Page loaded successfully. Inspect the parsed URL parts below.
        </p>
      </div>

      <div className="panel" style={{ padding: 0, overflow: 'hidden' }}>
        <div className="navtest__row">
          <div className="panel__label" style={{ marginBottom: 6 }}>Full URL</div>
          <div className="navtest__val">
            {fullUrl || <span style={{ color: 'var(--ink-4)', fontStyle: 'italic' }}>—</span>}
          </div>
        </div>

        <div className="navtest__row">
          <div className="panel__label" style={{ marginBottom: 6 }}>Query Params</div>
          {params.length === 0 ? (
            <span style={{ color: 'var(--ink-4)', fontStyle: 'italic', fontSize: 13 }}>none</span>
          ) : (
            params.map(([k, v]) => (
              <div key={k} className="navtest__kv">
                <span className="navtest__key">{k}</span>
                <span className="navtest__eq">=</span>
                <span className="navtest__val">{v}</span>
              </div>
            ))
          )}
        </div>

        <div className="navtest__row">
          <div className="panel__label" style={{ marginBottom: 6 }}>Fragment</div>
          {!fragment ? (
            <span style={{ color: 'var(--ink-4)', fontStyle: 'italic', fontSize: 13 }}>none</span>
          ) : fragmentParts.length === 1 && !fragmentParts[0].includes('=') ? (
            <span className="navtest__val">{fragment}</span>
          ) : (
            fragmentParts.map((part, i) => {
              const [k, ...rest] = part.split('=');
              return (
                <div key={i} className="navtest__kv">
                  <span className="navtest__key">{k}</span>
                  {rest.length > 0 && (
                    <>
                      <span className="navtest__eq">=</span>
                      <span className="navtest__val">{rest.join('=')}</span>
                    </>
                  )}
                </div>
              );
            })
          )}
        </div>
      </div>

      <div style={{ marginTop: 24 }}>
        <Link href="/" className="back" style={{ display: 'inline-flex' }}>
          ← Back to playground
        </Link>
      </div>
    </div>
  );
}

export default function NavigationTestPage() {
  return (
    <div className="shell">
      <Suspense>
        <PageContent />
      </Suspense>
    </div>
  );
}
