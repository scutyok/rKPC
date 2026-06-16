use crate::util::math::Vector3;

#[derive(Clone, Copy, Debug)]
pub struct AABB {
	pub min: Vector3,
	pub max: Vector3,
}

impl AABB {
	pub fn contains(&self, point: Vector3) -> bool {
		point.x >= self.min.x && point.x <= self.max.x &&
		point.y >= self.min.y && point.y <= self.max.y &&
		point.z >= self.min.z && point.z <= self.max.z
	}

	pub fn intersects(&self, other: &AABB) -> bool {
		!(self.max.x < other.min.x || self.min.x > other.max.x ||
		  self.max.y < other.min.y || self.min.y > other.max.y ||
		  self.max.z < other.min.z || self.min.z > other.max.z)
	}

	// Line-segment / AABB intersection using the separating axis theorem.
	pub fn intersects_line_segment(&self, l0: Vector3, l1: Vector3) -> bool {
		let l = l1 - l0;
		let t = (l0 + l1) - (self.max + self.min);
		let e = self.max - self.min;

		if t.x.abs() > l.x.abs() + e.x { return false; }
		if t.y.abs() > l.y.abs() + e.y { return false; }
		if t.z.abs() > l.z.abs() + e.z { return false; }

		if (t.y * l.z - t.z * l.y).abs() > e.y * l.z.abs() + e.z * l.y.abs() { return false; }
		if (t.z * l.x - t.x * l.z).abs() > e.x * l.z.abs() + e.z * l.x.abs() { return false; }
		if (t.x * l.y - t.y * l.x).abs() > e.x * l.y.abs() + e.y * l.x.abs() { return false; }

		true
	}

	pub fn compute_tri_aabb(pos: &[Vector3], idx: &[u32], t: usize) -> AABB {
        let a = pos[idx[t * 3] as usize];
        let b = pos[idx[t * 3 + 1] as usize];
        let c = pos[idx[t * 3 + 2] as usize];
        
        AABB {
            min: Vector3::new(a.x.min(b.x).min(c.x), a.y.min(b.y).min(c.y), a.z.min(b.z).min(c.z)),
            max: Vector3::new(a.x.max(b.x).max(c.x), a.y.max(b.y).max(c.y), a.z.max(b.z).max(c.z)),
        }
    }

    pub fn merge_aabb(a: &AABB, b: &AABB) -> AABB {
        AABB {
            min: Vector3::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y), a.min.z.min(b.min.z)),
            max: Vector3::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y), a.max.z.max(b.max.z)),
        }
    }

    pub fn compute_centroid(pos: &[Vector3], idx: &[u32], t: usize) -> Vector3 {
        let a = pos[idx[t * 3] as usize];
        let b = pos[idx[t * 3 + 1] as usize];
        let c = pos[idx[t * 3 + 2] as usize];
        (a + b + c) * (1.0 / 3.0)
    }
}
