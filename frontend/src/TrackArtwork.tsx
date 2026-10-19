import { memo, useState } from "react";
import { Music2 } from "lucide-react";

function TrackArtwork({
  localId,
  url,
}: {
  localId?: string | number;
  url?: string | null;
}) {
  const src =
    url ||
    (localId !== undefined
      ? `pulsedeck-art://localhost/${encodeURIComponent(localId)}`
      : "");
  const [failed, setFailed] = useState("");
  return (
    <div className="track-art">
      {src && failed !== src ? (
        <img
          src={src}
          width={30}
          height={30}
          loading="lazy"
          decoding="async"
          alt=""
          onError={() => setFailed(src)}
        />
      ) : (
        <Music2 size={15} />
      )}
    </div>
  );
}
export default memo(TrackArtwork);
