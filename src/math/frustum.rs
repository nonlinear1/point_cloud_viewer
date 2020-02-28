use super::base::PointCulling;
use super::sat::{ConvexPolyhedron, Intersector};
use arrayvec::ArrayVec;
use nalgebra::{Isometry3, Matrix4, Point3, RealField, Unit, Vector3};
use serde::{Deserialize, Serialize};

pub mod collision {
    use nalgebra::{Matrix4, RealField};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Perspective<S: RealField> {
        matrix: Matrix4<S>,
    }

    impl<S: RealField> Perspective<S> {
        pub fn new(left: S, right: S, bottom: S, top: S, near: S, far: S) -> Self {
            assert!(
                left <= right,
                "`left` cannot be greater than `right`, found: left: {:?} right: {:?}",
                left,
                right
            );
            assert!(
                bottom <= top,
                "`bottom` cannot be greater than `top`, found: bottom: {:?} top: {:?}",
                bottom,
                top
            );
            assert!(
                near <= far,
                "`near` cannot be greater than `far`, found: near: {:?} far: {:?}",
                near,
                far
            );

            let two: S = nalgebra::convert(2.0);

            let c0r0 = (two * near) / (right - left);
            let c0r1 = nalgebra::zero();
            let c0r2 = nalgebra::zero();
            let c0r3 = nalgebra::zero();

            let c1r0 = nalgebra::zero();
            let c1r1 = (two * near) / (top - bottom);
            let c1r2 = nalgebra::zero();
            let c1r3 = nalgebra::zero();

            let c2r0 = (right + left) / (right - left);
            let c2r1 = (top + bottom) / (top - bottom);
            let c2r2 = -(far + near) / (far - near);
            let c2r3 = -S::one();

            let c3r0 = nalgebra::zero();
            let c3r1 = nalgebra::zero();
            let c3r2 = -(two * far * near) / (far - near);
            let c3r3 = nalgebra::zero();

            #[cfg_attr(rustfmt, rustfmt_skip)]
                let matrix = Matrix4::new(
                    c0r0, c0r1, c0r2, c0r3,
                    c1r0, c1r1, c1r2, c1r3,
                    c2r0, c2r1, c2r2, c2r3,
                    c3r0, c3r1, c3r2, c3r3,
                );
            Self { matrix }
        }

        // This emulates cgmath::PerspectiveFov, which more restricted
        // and corresponds to nalgebra::Perspective.
        pub fn new_fov(fovy: S, aspect: S, near: S, far: S) -> Self {
            let angle = nalgebra::convert::<f64, S>(0.5) * fovy;
            let ymax = near * angle.tan();
            let xmax = ymax * aspect;

            Self::new(-xmax, xmax, -ymax, ymax, near, far)
        }

        pub fn as_matrix(&self) -> &Matrix4<S> {
            &self.matrix
        }

        pub fn inverse(&self) -> Matrix4<S> {
            let c0r0 = self.matrix[(0, 0)].recip();
            let c0r1 = nalgebra::zero();
            let c0r2 = nalgebra::zero();
            let c0r3 = nalgebra::zero();

            let c1r0 = nalgebra::zero();
            let c1r1 = self.matrix[(1, 1)].recip();
            let c1r2 = nalgebra::zero();
            let c1r3 = nalgebra::zero();

            let c2r0 = nalgebra::zero();
            let c2r1 = nalgebra::zero();
            let c2r2 = nalgebra::zero();
            let c2r3 = self.matrix[(3, 2)].recip();

            let c3r0 = self.matrix[(2, 0)] / self.matrix[(0, 0)];
            let c3r1 = self.matrix[(2, 1)] / self.matrix[(1, 1)];
            let c3r2 = -S::one();
            let c3r3 = self.matrix[(2, 2)] / self.matrix[(3, 2)];

            #[cfg_attr(rustfmt, rustfmt_skip)]
            Matrix4::new(
                c0r0, c0r1, c0r2, c0r3,
                c1r0, c1r1, c1r2, c1r3,
                c2r0, c2r1, c2r2, c2r3,
                c3r0, c3r1, c3r2, c3r3,
            )
        }
    }
}

fn contains_point<S: RealField>(matrix: &Matrix4<S>, point: &Point3<S>) -> bool {
    let p_clip = matrix.transform_point(point);
    p_clip.coords.min() > nalgebra::convert(-1.0) && p_clip.coords.max() < nalgebra::convert(1.0)
}

/// A frustum is defined in eye coordinates, where x points right, y points up,
/// and z points against the viewing direction. This is not how e.g. OpenCV
/// defines a camera coordinate system. To get from OpenCV camera coordinates
/// to eye coordinates, you need to rotate 180 deg around the x axis before
/// creating the perspective projection, see also the frustum unit test below.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frustum<S: RealField> {
    query_from_clip: Matrix4<S>,
    clip_from_query: Matrix4<S>,
}

impl<S: RealField> Frustum<S> {
    pub fn new(query_from_eye: Isometry3<S>, clip_from_eye: collision::Perspective<S>) -> Self {
        let clip_from_query = clip_from_eye.as_matrix() * query_from_eye.inverse().to_homogeneous();
        let query_from_clip = query_from_eye.to_homogeneous() * clip_from_eye.inverse();
        Frustum {
            query_from_clip,
            clip_from_query,
        }
    }

    /// Fails if the matrix is not invertible.
    pub fn from_matrix4(clip_from_query: Matrix4<S>) -> Option<Self> {
        let query_from_clip = clip_from_query.try_inverse()?;
        Some(Self {
            query_from_clip,
            clip_from_query,
        })
    }
}

impl<S> PointCulling<S> for Frustum<S>
where
    S: RealField,
{
    fn contains(&self, point: &Point3<S>) -> bool {
        contains_point(&self.clip_from_query, point)
    }
}

impl<S> ConvexPolyhedron<S> for Frustum<S>
where
    S: RealField,
{
    fn compute_corners(&self) -> [Point3<S>; 8] {
        let corner_from = |x, y, z| self.query_from_clip.transform_point(&Point3::new(x, y, z));
        [
            corner_from(-S::one(), -S::one(), -S::one()),
            corner_from(-S::one(), -S::one(), S::one()),
            corner_from(-S::one(), S::one(), -S::one()),
            corner_from(-S::one(), S::one(), S::one()),
            corner_from(S::one(), -S::one(), -S::one()),
            corner_from(S::one(), -S::one(), S::one()),
            corner_from(S::one(), S::one(), -S::one()),
            corner_from(S::one(), S::one(), S::one()),
        ]
    }

    fn compute_edges(&self) -> ArrayVec<[Unit<Vector3<S>>; 6]> {
        // To compute the edges, we need the points, so it's more efficient to implement
        // intersector() directly and compute the points only once. We still provide this
        // function, but it will not be used since intersection testing only needs
        // intersector().
        self.intersector().edges
    }

    fn compute_face_normals(&self) -> ArrayVec<[Unit<Vector3<S>>; 6]> {
        // To compute the face normals, we need the edges, so it's more efficient to
        // implement intersector() directly and compute the points and edges only once.
        // We still provide this function, but it will not be used since intersection
        // testing only needs intersector().
        self.intersector().face_normals
    }

    fn intersector(&self) -> Intersector<S> {
        let corners = self.compute_corners();

        let edges = ArrayVec::from([
            Unit::new_normalize(corners[4] - corners[0]), // x
            Unit::new_normalize(corners[2] - corners[0]), // y
            Unit::new_normalize(corners[1] - corners[0]), // z lower left
            Unit::new_normalize(corners[3] - corners[2]), // z upper left
            Unit::new_normalize(corners[5] - corners[4]), // z lower right
            Unit::new_normalize(corners[7] - corners[6]), // z upper right
        ]);

        let mut face_normals = ArrayVec::new();
        face_normals.push(Unit::new_normalize(edges[0].cross(&edges[1]))); // Front and back sides
        face_normals.push(Unit::new_normalize(edges[0].cross(&edges[2]))); // Lower side
        face_normals.push(Unit::new_normalize(edges[0].cross(&edges[3]))); // Upper side
        face_normals.push(Unit::new_normalize(edges[1].cross(&edges[2]))); // Left side
        face_normals.push(Unit::new_normalize(edges[1].cross(&edges[2]))); // right side

        Intersector {
            corners,
            edges,
            face_normals,
        }
    }
}