import { createContext, useContext } from 'react';
import type { VersionMeta, GroupDef, MethodDef, DataType } from '../data/registry';

interface VersionContextValue {
  version: VersionMeta;
  groups: GroupDef[];
  methods: MethodDef[];
  dataTypes: DataType[];
  getTypeById: (id: string) => DataType | undefined;
  getMethodById: (id: string) => MethodDef | undefined;
  getGroupById: (id: string) => GroupDef | undefined;
  /** URL prefix for this version, e.g. "/v/0.1" */
  versionPrefix: string;
}

const VersionContext = createContext<VersionContextValue | null>(null);

export function useVersion(): VersionContextValue {
  const ctx = useContext(VersionContext);
  if (!ctx) throw new Error('useVersion must be used within VersionProvider');
  return ctx;
}

export function VersionProvider({
  version,
  children,
}: {
  version: VersionMeta;
  children: React.ReactNode;
}) {
  const value: VersionContextValue = {
    version,
    groups: version.data.groups,
    methods: version.data.methods,
    dataTypes: version.data.dataTypes,
    getTypeById: version.data.getTypeById,
    getMethodById: version.data.getMethodById,
    getGroupById: version.data.getGroupById,
    versionPrefix: `/v/${version.slug}`,
  };

  return (
    <VersionContext.Provider value={value}>{children}</VersionContext.Provider>
  );
}
