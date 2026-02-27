/**
 * Effects Manager - Engagements, missiles, and explosions
 */

export class EffectsManager {
    constructor() {
        this.engagements = new Map();
        this.explosions = [];
        this.explosionDuration = 1000; // ms
    }

    /**
     * Update or add an engagement
     */
    updateEngagement(engagementData) {
        this.engagements.set(engagementData.engagement_id, {
            ...engagementData,
            lastUpdate: Date.now(),
        });
    }

    /**
     * Remove completed engagement
     */
    removeEngagement(engagementId) {
        this.engagements.delete(engagementId);
    }

    /**
     * Get all active engagements
     */
    getEngagements() {
        return Array.from(this.engagements.values());
    }

    /**
     * Get engagement count
     */
    getEngagementCount() {
        return this.engagements.size;
    }

    /**
     * Create explosion at position
     */
    createExplosion(pos) {
        if (!pos) return;

        this.explosions.push({
            pos: pos,
            startTime: Date.now(),
            progress: 0,
        });
    }

    /**
     * Update explosion animations
     */
    updateExplosions() {
        const now = Date.now();

        for (let i = this.explosions.length - 1; i >= 0; i--) {
            const explosion = this.explosions[i];
            const elapsed = now - explosion.startTime;
            explosion.progress = elapsed / this.explosionDuration;

            // Remove finished explosions
            if (explosion.progress >= 1) {
                this.explosions.splice(i, 1);
            }
        }
    }

    /**
     * Get active explosions
     */
    getExplosions() {
        return this.explosions;
    }
}
