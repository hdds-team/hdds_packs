/**
 * Track Manager - Manages aircraft tracks
 */

export class TrackManager {
    constructor() {
        this.tracks = new Map();
        this.trackHistory = new Map(); // Position history for trails
        this.maxHistoryLength = 20;
    }

    /**
     * Update or add a track
     */
    updateTrack(trackData) {
        const id = trackData.track_id;

        // Store track
        this.tracks.set(id, {
            ...trackData,
            lastUpdate: Date.now(),
        });

        // Update position history
        if (!this.trackHistory.has(id)) {
            this.trackHistory.set(id, []);
        }

        const history = this.trackHistory.get(id);
        history.push({
            x: trackData.x_km,
            y: trackData.y_km,
            time: Date.now(),
        });

        // Keep history limited
        while (history.length > this.maxHistoryLength) {
            history.shift();
        }
    }

    /**
     * Remove a track (destroyed or left area)
     */
    removeTrack(trackId) {
        this.tracks.delete(trackId);
        this.trackHistory.delete(trackId);
    }

    /**
     * Get all current tracks
     */
    getTracks() {
        const now = Date.now();
        const activeTracks = [];

        for (const [id, track] of this.tracks) {
            // Remove stale tracks (no update in 5 seconds)
            if (now - track.lastUpdate > 5000) {
                this.tracks.delete(id);
                this.trackHistory.delete(id);
                continue;
            }
            activeTracks.push(track);
        }

        return activeTracks;
    }

    /**
     * Get track by ID
     */
    getTrack(trackId) {
        return this.tracks.get(trackId);
    }

    /**
     * Get track position (for explosion placement)
     */
    getTrackPosition(trackId) {
        const track = this.tracks.get(trackId);
        if (track) {
            return { x: track.x_km, y: track.y_km };
        }
        return null;
    }

    /**
     * Get position history for a track
     */
    getTrackHistory(trackId) {
        return this.trackHistory.get(trackId) || [];
    }

    /**
     * Get track count by status
     */
    getCountByStatus(status) {
        let count = 0;
        for (const track of this.tracks.values()) {
            if (track.status === status) count++;
        }
        return count;
    }
}
