import { memo, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { listen } from "@tauri-apps/api/event";
import { control, isDesktop } from "./api";

export interface LibraryFilters {
  my_tags?: number[];
  tag_mode?: "and" | "or";
  rating_min?: number;
  rating_max?: number;
  colors?: string[];
}
type MyTag = { id: number; name: string; position: number; count: number };
type Category = { id: number; name: string; position: number; tags: MyTag[] };
type EmbeddedTag = {
  format: string;
  key: string;
  standard_key: string | null;
  value: string;
  description: string;
  language: string;
};
type Details = {
  id: number;
  revision: number;
  title: string;
  artist: string;
  fields: Record<string, string>;
  rating: number;
  color: string;
  my_tags: number[];
  file_metadata: EmbeddedTag[];
  has_file_edits: boolean;
};
export const trackColors = [
  ["", "No color"],
  ["#FF007F", "Rose"],
  ["#FF0000", "Red"],
  ["#FFA500", "Orange"],
  ["#FFFF00", "Lemon"],
  ["#00FF00", "Green"],
  ["#25FDE9", "Turquoise"],
  ["#0000FF", "Blue"],
  ["#660099", "Violet"],
];
const standardFields = [
  ["title", "Title"],
  ["artist", "Artist"],
  ["AlbumTitle", "Album"],
  ["AlbumArtist", "Album artist"],
  ["Genre", "Genre"],
  ["Comment", "Comments"],
  ["Label", "Label"],
  ["ContentGroup", "Grouping"],
  ["Composer", "Composer"],
  ["Remixer", "Remixer"],
  ["Year", "Year"],
  ["TrackNumber", "Track number"],
  ["DiscNumber", "Disc number"],
  ["Isrc", "ISRC"],
];

export default memo(function LibraryTools({
  selectedId,
  filters,
  onFilters,
  onSaved,
}: {
  selectedId: string | number | null;
  filters: LibraryFilters;
  onFilters: (value: LibraryFilters) => void;
  onSaved: () => void;
}) {
  const [open, setOpen] = useState(false),
    [editor, setEditor] = useState<string | number | null>(null);
  const [categories, setCategories] = useState<Category[]>([]),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  const [categoryDraft, setCategoryDraft] = useState(""),
    [tagDraft, setTagDraft] = useState<Record<number, string>>({});
  const refreshCategories = () =>
    control<Category[]>("library.tag_categories")
      .then(setCategories)
      .catch((e) => setError(String(e)));
  useEffect(() => {
    if (!isDesktop) return;
    void refreshCategories();
    let gone = false;
    let stop: (() => void) | undefined;
    void listen<{ command?: string }>("library-updated", (event) => {
      if (
        [
          "library.edit_taxonomy",
          "library.edit_details",
          "library.remove",
          "library.import_xml",
        ].includes(event.payload?.command || "")
      )
        void refreshCategories();
    }).then((fn) => {
      if (gone) fn();
      else stop = fn;
    });
    return () => {
      gone = true;
      stop?.();
    };
  }, []);
  async function taxonomy(args: Record<string, unknown>) {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      const next = await control<Category[]>("library.edit_taxonomy", args);
      setCategories(next);
      const ids = new Set(next.flatMap((c) => c.tags.map((t) => t.id)));
      onFilters({
        ...filters,
        my_tags: (filters.my_tags || []).filter((id) => ids.has(id)),
      });
      onSaved();
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    } finally {
      setBusy(false);
    }
  }
  const active = !!(
    filters.my_tags?.length ||
    filters.colors?.length ||
    filters.rating_min !== undefined ||
    filters.rating_max !== undefined
  );
  return (
    <div className="library-tools">
      <div className="library-tools-actions">
        <button aria-expanded={open} onClick={() => setOpen(!open)}>
          Filters / My Tags{active ? " •" : ""}
        </button>
        <button
          disabled={selectedId === null}
          onClick={() => setEditor(selectedId)}
        >
          Edit metadata
        </button>
        {active && <button onClick={() => onFilters({})}>Clear filters</button>}
      </div>
      {open && (
        <div className="library-filter-panel" aria-label="Library filters">
          <div className="library-filter-ranges">
            <label>
              Match My Tags{" "}
              <select
                aria-label="My Tag match mode"
                value={filters.tag_mode || "and"}
                onChange={(e) =>
                  onFilters({
                    ...filters,
                    tag_mode: e.target.value as "and" | "or",
                  })
                }
              >
                <option value="and">All selected (AND)</option>
                <option value="or">Any selected (OR)</option>
              </select>
            </label>
            <label>
              Minimum rating{" "}
              <select
                aria-label="Minimum rating"
                value={filters.rating_min ?? ""}
                onChange={(e) =>
                  onFilters({
                    ...filters,
                    rating_min:
                      e.target.value === ""
                        ? undefined
                        : Number(e.target.value),
                  })
                }
              >
                <option value="">Any</option>
                {[0, 1, 2, 3, 4, 5].map((n) => (
                  <option key={n} value={n}>
                    {n} stars
                  </option>
                ))}
              </select>
            </label>
            <label>
              Maximum rating{" "}
              <select
                aria-label="Maximum rating"
                value={filters.rating_max ?? ""}
                onChange={(e) =>
                  onFilters({
                    ...filters,
                    rating_max:
                      e.target.value === ""
                        ? undefined
                        : Number(e.target.value),
                  })
                }
              >
                <option value="">Any</option>
                {[0, 1, 2, 3, 4, 5].map((n) => (
                  <option key={n} value={n}>
                    {n} stars
                  </option>
                ))}
              </select>
            </label>
            <fieldset className="color-filters">
              <legend>Colors (any selected)</legend>
              {[
                ...trackColors,
                ...(filters.colors || [])
                  .filter((c) => !trackColors.some(([v]) => v === c))
                  .map((c) => [c, c]),
              ].map(([color, name]) => (
                <label key={color} title={name}>
                  <input
                    type="checkbox"
                    aria-label={`Filter color ${name}`}
                    checked={filters.colors?.includes(color) || false}
                    onChange={(e) =>
                      onFilters({
                        ...filters,
                        colors: e.target.checked
                          ? [...(filters.colors || []), color]
                          : (filters.colors || []).filter((c) => c !== color),
                      })
                    }
                  />
                  <span
                    className="track-color"
                    style={{ background: color || "transparent" }}
                  />
                  {name}
                </label>
              ))}
              <label>
                Custom color
                <input
                  type="color"
                  aria-label="Filter custom color"
                  onChange={(e) =>
                    onFilters({
                      ...filters,
                      colors: [
                        ...new Set([
                          ...(filters.colors || []),
                          e.target.value.toUpperCase(),
                        ]),
                      ],
                    })
                  }
                />
              </label>
            </fieldset>
          </div>
          <div className="my-tag-categories">
            {categories.map((c) => (
              <details
                key={c.id}
                open={!!c.tags.some((t) => filters.my_tags?.includes(t.id))}
              >
                <summary>
                  {c.name} · {c.tags.length}
                </summary>
                <div className="my-tag-options">
                  {c.tags.map((t) => (
                    <label key={t.id}>
                      <input
                        type="checkbox"
                        aria-label={`Filter ${c.name}: ${t.name}`}
                        checked={filters.my_tags?.includes(t.id) || false}
                        onChange={(e) =>
                          onFilters({
                            ...filters,
                            my_tags: e.target.checked
                              ? [...(filters.my_tags || []), t.id]
                              : (filters.my_tags || []).filter(
                                  (id) => id !== t.id,
                                ),
                          })
                        }
                      />
                      {t.name}
                      <small>{t.count}</small>
                    </label>
                  ))}
                </div>
              </details>
            ))}
          </div>
          <details className="taxonomy-manager">
            <summary>Manage categories and tags</summary>
            <form
              onSubmit={(e) => {
                e.preventDefault();
                void taxonomy({ kind: "category", name: categoryDraft }).then(
                  (ok) => {
                    if (ok) setCategoryDraft("");
                  },
                );
              }}
            >
              <input
                aria-label="New category name"
                maxLength={128}
                value={categoryDraft}
                onChange={(e) => setCategoryDraft(e.target.value)}
                placeholder="New category…"
              />
              <button disabled={busy || !categoryDraft.trim()}>
                Add category
              </button>
            </form>
            {categories.map((c, index) => (
              <details key={c.id}>
                <summary>{c.name}</summary>
                <TaxonomyName
                  kind="category"
                  item={c}
                  busy={busy}
                  save={taxonomy}
                />
                <button
                  disabled={busy || index === 0}
                  onClick={() =>
                    void taxonomy({
                      kind: "category",
                      id: c.id,
                      name: c.name,
                      position: categories[index - 1].position - 1,
                    })
                  }
                >
                  Move category up
                </button>
                {c.tags.map((t, i) => (
                  <div className="taxonomy-tag" key={t.id}>
                    <TaxonomyName
                      kind="tag"
                      item={t}
                      busy={busy}
                      save={taxonomy}
                    />
                    <button
                      disabled={busy || i === 0}
                      onClick={() =>
                        void taxonomy({
                          kind: "tag",
                          id: t.id,
                          name: t.name,
                          position: c.tags[i - 1].position - 1,
                        })
                      }
                    >
                      Move up
                    </button>
                  </div>
                ))}
                <form
                  onSubmit={(e) => {
                    e.preventDefault();
                    void taxonomy({
                      kind: "tag",
                      category: c.id,
                      name: tagDraft[c.id] || "",
                    }).then((ok) => {
                      if (ok) setTagDraft((v) => ({ ...v, [c.id]: "" }));
                    });
                  }}
                >
                  <input
                    aria-label={`New tag in ${c.name}`}
                    placeholder="New tag…"
                    maxLength={128}
                    value={tagDraft[c.id] || ""}
                    onChange={(e) =>
                      setTagDraft((v) => ({ ...v, [c.id]: e.target.value }))
                    }
                  />
                  <button disabled={busy || !tagDraft[c.id]?.trim()}>
                    Add tag
                  </button>
                </form>
              </details>
            ))}
          </details>
        </div>
      )}
      {error && <p role="alert">{error}</p>}
      {editor !== null && (
        <MetadataEditor
          id={editor}
          categories={categories}
          close={() => setEditor(null)}
          saved={onSaved}
        />
      )}
    </div>
  );
});
function TaxonomyName({
  kind,
  item,
  busy,
  save,
}: {
  kind: string;
  item: { id: number; name: string };
  busy: boolean;
  save: (args: Record<string, unknown>) => Promise<boolean | undefined>;
}) {
  const [name, setName] = useState(item.name),
    [remove, setRemove] = useState(false);
  useEffect(() => setName(item.name), [item.name]);
  return (
    <span className="taxonomy-name">
      <input
        aria-label={`Rename ${kind} ${item.name}`}
        maxLength={128}
        value={name}
        onChange={(e) => setName(e.target.value)}
      />
      <button
        disabled={busy || name.trim() === item.name || !name.trim()}
        onClick={() => void save({ kind, id: item.id, name })}
      >
        Rename
      </button>
      {remove ? (
        <>
          <span>Remove this {kind} and its assignments?</span>
          <button
            disabled={busy}
            onClick={() => void save({ kind, id: item.id, delete: true })}
          >
            Remove
          </button>
          <button onClick={() => setRemove(false)}>Cancel</button>
        </>
      ) : (
        <button disabled={busy} onClick={() => setRemove(true)}>
          Delete
        </button>
      )}
    </span>
  );
}
function MetadataEditor({
  id,
  categories,
  close,
  saved,
}: {
  id: string | number;
  categories: Category[];
  close: () => void;
  saved: () => void;
}) {
  const [details, setDetails] = useState<Details | null>(null),
    [draft, setDraft] = useState<Record<string, string>>({}),
    [rating, setRating] = useState(0),
    [color, setColor] = useState(""),
    [tags, setTags] = useState<number[]>([]),
    [custom, setCustom] = useState(""),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [message, setMessage] = useState("");
  const dialog = useRef<HTMLDialogElement>(null),
    initialFocus = useRef<HTMLButtonElement>(null);
  function hydrate(value: Details) {
    setDetails(value);
    setDraft({});
    setRating(value.rating);
    setColor(value.color);
    setTags(value.my_tags);
  }
  useEffect(() => {
    let active = true;
    void control<Details>("library.details", { id })
      .then((d) => {
        if (active) hydrate(d);
      })
      .catch((e) => {
        if (active) setError(String(e));
      });
    return () => {
      active = false;
    };
  }, [id]);
  useEffect(() => {
    const focused = document.activeElement as HTMLElement | null;
    dialog.current?.showModal();
    initialFocus.current?.focus();
    return () => {
      focused?.focus();
    };
  }, []);
  const value = (key: string) => draft[key] ?? details?.fields[key] ?? "";
  const dirty =
    Object.keys(draft).length > 0 ||
    rating !== details?.rating ||
    color !== details?.color ||
    JSON.stringify(tags) !== JSON.stringify(details?.my_tags);
  async function save() {
    if (!details || busy) return;
    setBusy(true);
    setError("");
    try {
      hydrate(
        await control<Details>("library.edit_details", {
          id,
          patch: {
            revision: details.revision,
            fields: draft,
            rating,
            color,
            my_tags: tags,
          },
        }),
      );
      setMessage("Saved in library. Audio analysis and cues retained.");
      saved();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  async function write() {
    if (!details || busy || dirty) return;
    setBusy(true);
    setError("");
    try {
      const result = await control<{ backup: string; details: Details }>(
        "library.write_metadata",
        { id, revision: details.revision },
      );
      hydrate(result.details);
      setMessage(`File tags written. Original backup: ${result.backup}`);
      saved();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  return createPortal(
    <dialog
      ref={dialog}
      className="metadata-dialog"
      aria-label="Edit track metadata"
      onCancel={(e) => {
        if (busy) e.preventDefault();
        else close();
      }}
      onKeyDown={(e) => e.stopPropagation()}
    >
      <div className="metadata-header">
        <h2>Track metadata</h2>
        <button
          ref={initialFocus}
          disabled={busy}
          onClick={close}
          aria-label="Close metadata editor"
        >
          ×
        </button>
      </div>
      {!details ? (
        <p>{error || "Reading saved metadata…"}</p>
      ) : (
        <>
          <p className="metadata-note">
            Library edits survive import and reanalysis. File writeback is a
            separate action with an original-file backup.
          </p>
          <div className="metadata-form">
            {standardFields.map(([key, label]) => (
              <label key={key}>
                {label}
                {key === "Comment" ? (
                  <textarea
                    aria-label={label}
                    value={value(key)}
                    onChange={(e) =>
                      setDraft((d) => ({ ...d, [key]: e.target.value }))
                    }
                  />
                ) : (
                  <input
                    aria-label={label}
                    value={value(key)}
                    onChange={(e) =>
                      setDraft((d) => ({ ...d, [key]: e.target.value }))
                    }
                  />
                )}
              </label>
            ))}
            <label>
              Rating
              <select
                aria-label="Track rating"
                value={rating}
                onChange={(e) => setRating(Number(e.target.value))}
              >
                {[0, 1, 2, 3, 4, 5].map((n) => (
                  <option key={n} value={n}>
                    {n === 0 ? "Unrated" : `${n} stars`}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Color
              <select
                aria-label="Track color"
                value={color}
                onChange={(e) => setColor(e.target.value)}
              >
                {color && !trackColors.some(([v]) => v === color) && (
                  <option value={color}>{color}</option>
                )}
                {trackColors.map(([v, name]) => (
                  <option key={v} value={v}>
                    {name}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <label>
            Custom track color
            <input
              type="color"
              aria-label="Custom track color"
              value={color || "#FF0000"}
              onChange={(e) => setColor(e.target.value.toUpperCase())}
            />
          </label>
          <fieldset className="metadata-my-tags">
            <legend>My Tags</legend>
            {categories.length === 0 ? (
              <p>Create categories and tags in Filters / My Tags.</p>
            ) : (
              categories.map((c) => (
                <details
                  key={c.id}
                  open={c.tags.some((t) => tags.includes(t.id))}
                >
                  <summary>{c.name}</summary>
                  {c.tags.map((t) => (
                    <label key={t.id}>
                      <input
                        type="checkbox"
                        aria-label={`Assign ${c.name}: ${t.name}`}
                        checked={tags.includes(t.id)}
                        onChange={(e) =>
                          setTags((v) =>
                            e.target.checked
                              ? [...v, t.id]
                              : v.filter((id) => id !== t.id),
                          )
                        }
                      />
                      {t.name}
                    </label>
                  ))}
                </details>
              ))
            )}
          </fieldset>
          <details>
            <summary>Custom fields and embedded originals</summary>
            <label>
              Field key
              <input
                aria-label="Custom metadata key"
                list="metadata-keys"
                value={custom}
                maxLength={256}
                onChange={(e) => setCustom(e.target.value)}
              />
            </label>
            <datalist id="metadata-keys">
              {Object.keys(details.fields)
                .filter((k) => !standardFields.some(([key]) => key === k))
                .map((k) => (
                  <option key={k} value={k} />
                ))}
            </datalist>
            {custom.trim() && (
              <label>
                Value
                <textarea
                  aria-label="Custom metadata value"
                  value={value(custom.trim())}
                  onChange={(e) =>
                    setDraft((d) => ({ ...d, [custom.trim()]: e.target.value }))
                  }
                />
              </label>
            )}
            <p>
              Clear a value to hide it in the library. Embedded originals remain
              available below until explicit file writeback.
            </p>
            <details>
              <summary>
                Embedded source fields ({details.file_metadata.length})
              </summary>
              <pre>{JSON.stringify(details.file_metadata, null, 2)}</pre>
            </details>
          </details>
          <div className="metadata-footer">
            <button disabled={busy || !dirty} onClick={() => void save()}>
              Save to library
            </button>
            <button
              disabled={busy || dirty || !details.has_file_edits}
              onClick={() => void write()}
            >
              Write saved text edits to file
            </button>
            <button
              disabled={busy}
              onClick={() => {
                setError("");
                void control<Details>("library.details", { id })
                  .then(hydrate)
                  .catch((e) => setError(String(e)));
              }}
            >
              Reload / discard draft
            </button>
          </div>
          {message && <p role="status">{message}</p>}
          {error && <p role="alert">{error}</p>}
        </>
      )}
    </dialog>,
    document.body,
  );
}
