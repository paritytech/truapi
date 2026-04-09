import type { GroupDef, MethodDef, DataType } from './v01/types';
import * as v01 from './v01/types';
import * as v02 from './v02/types';

export type { GroupDef, MethodDef, DataType };

export interface VersionData {
  groups: GroupDef[];
  methods: MethodDef[];
  dataTypes: DataType[];
  getTypeById: (id: string) => DataType | undefined;
  getMethodById: (id: string) => MethodDef | undefined;
  getGroupById: (id: string) => GroupDef | undefined;
}

export interface VersionMeta {
  id: string;
  label: string;
  slug: string;
  status: 'stable' | 'preview';
  data: VersionData;
}

export const versions: VersionMeta[] = [
  {
    id: '0.1',
    label: 'v0.1',
    slug: '0.1',
    status: 'stable',
    data: v01,
  },
  {
    id: '0.2',
    label: 'v0.2',
    slug: '0.2',
    status: 'stable',
    data: v02,
  },
];

export const defaultVersion = versions[1];

export function getVersion(slug: string): VersionMeta | undefined {
  return versions.find(v => v.slug === slug);
}
