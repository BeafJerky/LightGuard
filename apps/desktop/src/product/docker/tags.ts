//! 版本列表的纯函数：展示名去掉 `library/`，过滤后只渲染前 200 个，
//! 避免 nginx 这类上千标签的镜像一次铺满整页。

export const TAG_RENDER_LIMIT = 200;

const VERSION_TAG = /^v?(\d+(?:\.\d+)*)/i;
const PURE_VERSION = /^v?\d+(?:\.\d+)*$/i;

export function displayRepo(repo: string): string {
  return repo.startsWith('library/') ? repo.slice('library/'.length) : repo;
}

function versionKey(tag: string): number[] | null {
  const match = VERSION_TAG.exec(tag);
  if (!match) return null;
  return match[1].split('.').map((part) => Number(part));
}

/** registry 的 tags/list 按字典序返回，旧版本在前；这里改成置顶 latest、其余版本从新到旧。 */
export function sortTags(tags: string[]): string[] {
  const plain: { tag: string; index: number }[] = [];
  const versioned: { tag: string; index: number; key: number[]; pure: boolean }[] = [];
  tags.forEach((tag, index) => {
    const key = versionKey(tag);
    if (key) versioned.push({ tag, index, key, pure: PURE_VERSION.test(tag) });
    else plain.push({ tag, index });
  });
  plain.sort((left, right) => {
    const leftLatest = left.tag.toLowerCase() === 'latest' ? 0 : 1;
    const rightLatest = right.tag.toLowerCase() === 'latest' ? 0 : 1;
    return leftLatest - rightLatest || left.index - right.index;
  });
  versioned.sort((left, right) => {
    const length = Math.max(left.key.length, right.key.length);
    for (let position = 0; position < length; position += 1) {
      const diff = (right.key[position] ?? 0) - (left.key[position] ?? 0);
      if (diff !== 0) return diff;
    }
    // 同版本时纯版本号（1.24.0）排在带后缀的变体（1.24.0-alpine）前面。
    const pure = Number(right.pure) - Number(left.pure);
    return pure || left.index - right.index;
  });
  return [...plain, ...versioned].map((item) => item.tag);
}

export function filterTags(tags: string[], query: string): string[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return tags;
  return tags.filter((tag) => tag.toLowerCase().includes(needle));
}

export interface TagView {
  matched: string[];
  shown: string[];
  truncated: boolean;
  total: number;
}

export function tagView(
  tags: string[],
  query: string,
  limit = TAG_RENDER_LIMIT,
): TagView {
  const matched = filterTags(sortTags(tags), query);
  return {
    matched,
    shown: matched.slice(0, limit),
    truncated: matched.length > limit,
    total: tags.length,
  };
}
