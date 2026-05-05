import type { ServiceInfo } from '@/src/lib/services';
import { isMethodSupported } from '@/src/lib/host-api-bridge';
import type { TestEntry } from '@/src/lib/auto-test';
import { AUTO_TEST_ID } from '@/src/lib/auto-test';

const KIND_LABEL: Record<string, string> = {
  unary: 'Req / Res',
  subscription: 'Subscription',
};

export function ServiceTable({
  services,
  activeMethod,
  testResults,
  onSelect,
}: {
  services: ServiceInfo[];
  activeMethod?: { service: string; method: string } | null;
  testResults?: Record<string, TestEntry>;
  onSelect: (service: string, method: string) => void;
}) {
  const isAutoTestActive = activeMethod?.service === AUTO_TEST_ID;

  let autoTestMark: string | null = null;
  if (testResults && Object.keys(testResults).length > 0) {
    const isRunning = Object.values(testResults).some((e) => e.status === 'running');
    if (isRunning) {
      autoTestMark = '…';
    } else {
      const pass = Object.values(testResults).filter((e) => e.status === 'pass').length;
      const fail = Object.values(testResults).filter((e) => e.status === 'fail').length;
      autoTestMark = `${pass}p · ${fail}f`;
    }
  }

  const autoTestMarkRunning =
    testResults != null && Object.values(testResults).some((e) => e.status === 'running');

  return (
    <>
      <button
        type="button"
        className="method method--autotest"
        data-active={isAutoTestActive}
        data-supported="true"
        onClick={() => onSelect(AUTO_TEST_ID, '')}
      >
        <div className="method__body">
          <div className="method__name">Auto-Test</div>
          <div className="method__meta">
            <span className="method__desc">Run all methods</span>
            {autoTestMark && (
              <span
                className="method__mark autotest__mark"
                data-running={autoTestMarkRunning}
              >
                {autoTestMark}
              </span>
            )}
          </div>
        </div>
      </button>
      <nav aria-label="Service methods">
        {services.map((svc, i) => {
          const index = String(i + 1).padStart(2, '0');
          return (
            <section key={svc.name} className="service" data-testid={`service-${svc.name}`}>
              <div className="service__head">
                <span className="service__index">{index}</span>
                <span className="service__name">{svc.name}</span>
                <span className="service__count">{svc.methods.length}</span>
              </div>
              <div>
                {svc.methods.map((m) => {
                  const supported = isMethodSupported(svc.name, m.name);
                  const isActive =
                    activeMethod?.service === svc.name && activeMethod?.method === m.name;
                  const testStatus = testResults?.[`${svc.name}/${m.name}`]?.status;
                  const showStatus = testStatus != null && testStatus !== 'idle';
                  return (
                    <button
                      key={m.name}
                      type="button"
                      className="method"
                      data-testid={`method-${svc.name}-${m.name}`}
                      data-active={isActive}
                      data-supported={supported}
                      data-test-status={showStatus ? testStatus : undefined}
                      onClick={() => onSelect(svc.name, m.name)}
                    >
                      <div className="method__body">
                        <div className="method__name">{m.name}</div>
                        <div className="method__meta">
                          {m.description && (
                            <span className="method__desc">{m.description}</span>
                          )}
                          <span className="method__mark" data-kind={m.type}>
                            {!supported ? 'n/a' : KIND_LABEL[m.type]}
                          </span>
                        </div>
                      </div>
                    </button>
                  );
                })}
              </div>
            </section>
          );
        })}
      </nav>
    </>
  );
}
