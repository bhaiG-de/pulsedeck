/** Extract an ID without ever allowing a user-supplied origin into an iframe. */
export function spotifyTrackId(input: string): string | null {
  const value = input.trim();
  const uri = /^spotify:track:([A-Za-z0-9]{22})$/.exec(value);
  if (uri) return uri[1];
  try {
    const url = new URL(value);
    if (
      url.protocol !== "https:" ||
      url.hostname !== "open.spotify.com" ||
      url.username ||
      url.password
    )
      return null;
    return /^\/track\/([A-Za-z0-9]{22})\/?$/.exec(url.pathname)?.[1] || null;
  } catch {
    return null;
  }
}

export function spotifyWidgetUrl(id: string): string | null {
  return /^[A-Za-z0-9]{22}$/.test(id)
    ? `https://open.spotify.com/embed/track/${id}`
    : null;
}
