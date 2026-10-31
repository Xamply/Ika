// Share pending/cache requests. Original full-resolution URLs never reach mini decoders.
export function createButtonPreviewCache(invoke, convertFileSrc) {
  const requests = new Map();
  return path => {
    if (!path) return Promise.resolve(null);
    if (!requests.has(path)) {
      requests.set(path, invoke('get_wallpaper_button_preview', { videoPath: path })
        .then(result => result ? convertFileSrc(result) : null)
        .catch(error => {
          console.warn('Could not prepare wallpaper button preview:', error);
          requests.delete(path); // transient filesystem errors can be retried
          return null;
        }));
    }
    return requests.get(path);
  };
}
