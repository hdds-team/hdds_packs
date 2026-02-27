/**
 * Radar Scope - Canvas 2D rendering
 */

export class RadarScope {
    constructor(canvas) {
        this.canvas = canvas;
        this.ctx = canvas.getContext('2d');
        this.size = canvas.width;
        this.center = this.size / 2;
        this.range = 200; // km
        this.scale = this.center / this.range;

        this.sweepAngle = 0;
        this.sweepHistory = []; // For phosphor trail effect
        this.samSites = new Map();

        // Colors
        this.colors = {
            background: '#0a1a0a',
            grid: 'rgba(0, 255, 0, 0.12)',
            gridMajor: 'rgba(0, 255, 0, 0.25)',
            sweep: '#00ff00',
            sweepGlow: 'rgba(0, 255, 0, 0.3)',
            blip: '#00ff00',
            hostile: '#ff3333',
            friendly: '#3366ff',
            neutral: '#ffff00',
            unknown: '#888888',
            samRange: 'rgba(255, 255, 0, 0.08)',
            samBorder: 'rgba(255, 255, 0, 0.3)',
            missile: '#ff0000',
            explosion: '#ff8800',
        };
    }

    resize(size) {
        this.size = size;
        this.center = size / 2;
        this.scale = this.center / this.range;
    }

    /**
     * Convert km coordinates to canvas pixels
     */
    kmToPixel(x_km, y_km) {
        return {
            x: this.center + x_km * this.scale,
            y: this.center - y_km * this.scale, // Invert Y for screen coords
        };
    }

    /**
     * Clear the canvas
     */
    clear() {
        this.ctx.fillStyle = this.colors.background;
        this.ctx.fillRect(0, 0, this.size, this.size);
    }

    /**
     * Draw range rings and compass grid
     */
    drawGrid() {
        const ctx = this.ctx;

        // Circular mask
        ctx.save();
        ctx.beginPath();
        ctx.arc(this.center, this.center, this.center - 2, 0, Math.PI * 2);
        ctx.clip();

        // Range rings (50km intervals)
        for (let r = 50; r <= this.range; r += 50) {
            const radius = r * this.scale;
            ctx.beginPath();
            ctx.arc(this.center, this.center, radius, 0, Math.PI * 2);
            ctx.strokeStyle = r % 100 === 0 ? this.colors.gridMajor : this.colors.grid;
            ctx.lineWidth = r % 100 === 0 ? 1.5 : 0.5;
            ctx.stroke();

            // Range labels
            if (r % 100 === 0) {
                ctx.fillStyle = this.colors.grid;
                ctx.font = '10px Courier New';
                ctx.textAlign = 'center';
                ctx.fillText(`${r}km`, this.center, this.center - radius - 5);
            }
        }

        // Compass lines (30° intervals)
        for (let angle = 0; angle < 360; angle += 30) {
            const rad = angle * Math.PI / 180;
            const innerRadius = 20;
            const outerRadius = this.center - 2;

            ctx.beginPath();
            ctx.moveTo(
                this.center + Math.sin(rad) * innerRadius,
                this.center - Math.cos(rad) * innerRadius
            );
            ctx.lineTo(
                this.center + Math.sin(rad) * outerRadius,
                this.center - Math.cos(rad) * outerRadius
            );
            ctx.strokeStyle = angle % 90 === 0 ? this.colors.gridMajor : this.colors.grid;
            ctx.lineWidth = angle % 90 === 0 ? 1 : 0.5;
            ctx.stroke();

            // Compass labels
            if (angle % 90 === 0) {
                const labels = { 0: 'N', 90: 'E', 180: 'S', 270: 'W' };
                ctx.fillStyle = this.colors.gridMajor;
                ctx.font = 'bold 12px Courier New';
                ctx.textAlign = 'center';
                ctx.textBaseline = 'middle';
                const labelRadius = this.center - 15;
                ctx.fillText(
                    labels[angle],
                    this.center + Math.sin(rad) * labelRadius,
                    this.center - Math.cos(rad) * labelRadius
                );
            }
        }

        // Center marker
        ctx.beginPath();
        ctx.arc(this.center, this.center, 3, 0, Math.PI * 2);
        ctx.fillStyle = this.colors.sweep;
        ctx.fill();

        ctx.restore();
    }

    /**
     * Draw SAM site coverage circles
     */
    drawSamCoverage() {
        const ctx = this.ctx;

        for (const site of this.samSites.values()) {
            const pos = this.kmToPixel(site.x_km, site.y_km);
            const radius = site.range_km * this.scale;

            // Coverage area
            ctx.beginPath();
            ctx.arc(pos.x, pos.y, radius, 0, Math.PI * 2);
            ctx.fillStyle = this.colors.samRange;
            ctx.fill();
            ctx.strokeStyle = this.colors.samBorder;
            ctx.lineWidth = 1;
            ctx.stroke();

            // Site marker
            ctx.beginPath();
            ctx.arc(pos.x, pos.y, 5, 0, Math.PI * 2);
            ctx.fillStyle = site.active ? this.colors.samBorder : this.colors.hostile;
            ctx.fill();

            // Site label
            ctx.fillStyle = this.colors.samBorder;
            ctx.font = '9px Courier New';
            ctx.textAlign = 'center';
            ctx.fillText(site.site_name, pos.x, pos.y + 15);
        }
    }

    /**
     * Set sweep angle and update history
     */
    setSweepAngle(angle) {
        this.sweepAngle = angle;

        // Add to history for trail effect
        this.sweepHistory.push({
            angle: angle,
            time: Date.now(),
        });

        // Keep only last 2 seconds of history
        const now = Date.now();
        this.sweepHistory = this.sweepHistory.filter(h => now - h.time < 2000);
    }

    /**
     * Draw the rotating sweep line with phosphor trail
     */
    drawSweep() {
        const ctx = this.ctx;
        const rad = this.sweepAngle * Math.PI / 180;
        const sweepRadius = this.center - 2;

        // Draw phosphor trail (fading arc)
        ctx.save();
        ctx.beginPath();
        ctx.arc(this.center, this.center, this.center - 2, 0, Math.PI * 2);
        ctx.clip();

        // Trail gradient arc
        const trailLength = 60; // degrees
        const trailRad = trailLength * Math.PI / 180;
        const startRad = rad - Math.PI / 2 - trailRad;
        const endRad = rad - Math.PI / 2;

        const gradient = ctx.createConicGradient(endRad, this.center, this.center);
        gradient.addColorStop(0, 'rgba(0, 255, 0, 0.3)');
        gradient.addColorStop(trailRad / (Math.PI * 2), 'rgba(0, 255, 0, 0)');
        gradient.addColorStop(1, 'rgba(0, 255, 0, 0)');

        ctx.beginPath();
        ctx.moveTo(this.center, this.center);
        ctx.arc(this.center, this.center, sweepRadius, startRad, endRad);
        ctx.closePath();
        ctx.fillStyle = gradient;
        ctx.fill();

        // Main sweep line
        ctx.beginPath();
        ctx.moveTo(this.center, this.center);
        ctx.lineTo(
            this.center + Math.sin(rad) * sweepRadius,
            this.center - Math.cos(rad) * sweepRadius
        );
        ctx.strokeStyle = this.colors.sweep;
        ctx.lineWidth = 2;
        ctx.shadowColor = this.colors.sweep;
        ctx.shadowBlur = 10;
        ctx.stroke();
        ctx.shadowBlur = 0;

        ctx.restore();
    }

    /**
     * Draw an aircraft track
     */
    drawTrack(track) {
        const ctx = this.ctx;
        const pos = this.kmToPixel(track.x_km, track.y_km);

        // Check if in range
        const dist = Math.sqrt(track.x_km ** 2 + track.y_km ** 2);
        if (dist > this.range) return;

        // Get color based on status
        let color;
        switch (track.status) {
            case 'Hostile': color = this.colors.hostile; break;
            case 'Friendly': color = this.colors.friendly; break;
            case 'Neutral': color = this.colors.neutral; break;
            default: color = this.colors.unknown;
        }

        // Draw blip with glow
        ctx.save();
        ctx.shadowColor = color;
        ctx.shadowBlur = 8;

        // Aircraft symbol (triangle pointing in heading direction)
        const headingRad = track.heading_deg * Math.PI / 180;
        const size = 8;

        ctx.beginPath();
        ctx.translate(pos.x, pos.y);
        ctx.rotate(headingRad);

        // Triangle
        ctx.moveTo(0, -size);
        ctx.lineTo(-size * 0.6, size * 0.6);
        ctx.lineTo(size * 0.6, size * 0.6);
        ctx.closePath();

        ctx.fillStyle = color;
        ctx.fill();
        ctx.strokeStyle = color;
        ctx.lineWidth = 1;
        ctx.stroke();

        ctx.restore();

        // Callsign label
        ctx.fillStyle = color;
        ctx.font = '9px Courier New';
        ctx.textAlign = 'left';
        ctx.fillText(track.callsign, pos.x + 12, pos.y + 4);

        // Altitude label
        ctx.fillStyle = 'rgba(255, 255, 255, 0.5)';
        ctx.font = '8px Courier New';
        ctx.fillText(`FL${Math.round(track.altitude_m / 30.48 / 100)}`, pos.x + 12, pos.y + 14);
    }

    /**
     * Draw active engagement (missile in flight)
     */
    drawEngagement(engagement) {
        const ctx = this.ctx;
        const missilePos = this.kmToPixel(engagement.missile_x_km, engagement.missile_y_km);
        const targetPos = this.kmToPixel(engagement.intercept_x_km, engagement.intercept_y_km);

        // Line from missile to intercept point
        ctx.beginPath();
        ctx.moveTo(missilePos.x, missilePos.y);
        ctx.lineTo(targetPos.x, targetPos.y);
        ctx.strokeStyle = 'rgba(255, 0, 0, 0.5)';
        ctx.lineWidth = 1;
        ctx.setLineDash([4, 4]);
        ctx.stroke();
        ctx.setLineDash([]);

        // Missile marker
        ctx.beginPath();
        ctx.arc(missilePos.x, missilePos.y, 4, 0, Math.PI * 2);
        ctx.fillStyle = this.colors.missile;
        ctx.shadowColor = this.colors.missile;
        ctx.shadowBlur = 10;
        ctx.fill();
        ctx.shadowBlur = 0;

        // Time to intercept
        if (engagement.time_to_intercept_s > 0) {
            ctx.fillStyle = this.colors.missile;
            ctx.font = '8px Courier New';
            ctx.textAlign = 'center';
            ctx.fillText(
                `${engagement.time_to_intercept_s.toFixed(1)}s`,
                missilePos.x,
                missilePos.y - 10
            );
        }
    }

    /**
     * Draw explosion effect
     */
    drawExplosion(explosion) {
        if (!explosion.pos) return;

        const ctx = this.ctx;
        const pos = this.kmToPixel(explosion.pos.x, explosion.pos.y);
        const progress = explosion.progress;
        const radius = 5 + progress * 30;
        const alpha = 1 - progress;

        ctx.beginPath();
        ctx.arc(pos.x, pos.y, radius, 0, Math.PI * 2);
        ctx.fillStyle = `rgba(255, 136, 0, ${alpha * 0.5})`;
        ctx.fill();

        ctx.beginPath();
        ctx.arc(pos.x, pos.y, radius * 0.6, 0, Math.PI * 2);
        ctx.fillStyle = `rgba(255, 255, 0, ${alpha * 0.8})`;
        ctx.fill();
    }

    /**
     * Update SAM site data
     */
    updateSamSite(site) {
        this.samSites.set(site.site_id, site);
    }
}
