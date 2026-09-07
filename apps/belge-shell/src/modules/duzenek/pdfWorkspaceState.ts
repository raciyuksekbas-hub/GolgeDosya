/** Cumulative per-page rotation; changing selection never rewrites prior edits. */
export function rotatePages(previous: Record<string, number>, selected: string[], delta: -90 | 90): Record<string, number> {
    const next = { ...previous };
    for (const key of selected) next[key] = ((next[key] || 0) + delta + 360) % 360;
    return next;
}

export type PreviewMode = number | 'fit-page' | 'fit-width';
/** CSS geometry in pixels. One scale is applied to both axes, including rotation. */
export function previewGeometry(width: number, height: number, rotation: number, viewportWidth: number, viewportHeight: number, mode: PreviewMode) {
    const swapped = Math.abs(rotation % 180) === 90;
    const w = swapped ? height : width, h = swapped ? width : height;
    const availableWidth = Math.max(1, viewportWidth - 16), availableHeight = Math.max(1, viewportHeight - 16);
    const scale = typeof mode === 'number' ? mode / 100 : mode === 'fit-width' ? availableWidth / w : Math.min(availableWidth / w, availableHeight / h);
    return { width: w * scale, height: h * scale, imageWidth: width * scale, imageHeight: height * scale, scale };
}
