import { useState } from "react";

export default function TrackTags({
  title,
  tags,
  save,
}: {
  title: string;
  tags: string[];
  save: (tags: string[]) => Promise<void>;
}) {
  const [draft, setDraft] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  async function update(next: string[], adding = false) {
    if (saving) return;
    setSaving(true);
    setError("");
    try {
      await save(next);
      if (adding) setDraft("");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }
  function add() {
    const tag = draft.trim();
    if (!tag) return;
    if (tags.some((t) => t.toLowerCase() === tag.toLowerCase())) {
      setDraft("");
      return;
    }
    void update([...tags, tag], true);
  }
  return (
    <div
      className="track-tags"
      onClick={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      onDragStart={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
    >
      <div className="track-tag-list">
        {tags.map((tag) => (
          <span className="song-tag" key={tag}>
            <span>{tag}</span>
            <button
              type="button"
              aria-label={`Remove tag ${tag} from ${title}`}
              disabled={saving}
              onClick={() => void update(tags.filter((t) => t !== tag))}
            >
              ×
            </button>
          </span>
        ))}
      </div>
      <input
        aria-label={`Add tag to ${title}`}
        placeholder="Add tag…"
        title="Type a tag and press Enter"
        value={draft}
        maxLength={128}
        disabled={saving || tags.length >= 32}
        draggable={false}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Enter") {
            e.preventDefault();
            add();
          }
          if (e.key === "Escape") {
            e.preventDefault();
            setDraft("");
            setError("");
          }
        }}
      />
      {error && (
        <span className="track-tag-error" role="alert">
          {error}
        </span>
      )}
    </div>
  );
}
