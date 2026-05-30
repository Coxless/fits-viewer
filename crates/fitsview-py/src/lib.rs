use pyo3::prelude::*;
use pyo3::types::PyDict;
use numpy::{IntoPyArray, PyArray2};

use fitsview_core::{
    fits_reader::{load_fits, load_fits_hdu},
    stats::compute_stats,
    wcs::Wcs,
};

/// Python-accessible wrapper for a loaded FITS image.
#[pyclass(name = "FitsImage")]
struct PyFitsImage {
    path: std::path::PathBuf,
    hdu_index: usize,
}

#[pymethods]
impl PyFitsImage {
    /// Load header keywords as a Python dict.
    fn header(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let img = load_image(py, &self.path, self.hdu_index)?;
        let d = PyDict::new_bound(py);
        for (k, v) in &img.header {
            d.set_item(k, v)?;
        }
        Ok(d.into())
    }

    /// Return pixel data as a 2D numpy float32 array (height × width).
    fn data<'py>(&self, py: Python<'py>, hdu: Option<usize>) -> PyResult<Bound<'py, PyArray2<f32>>> {
        let hdu_idx = hdu.unwrap_or(self.hdu_index);
        let img = load_image(py, &self.path, hdu_idx)?;
        let h = img.height;
        let w = img.width;
        let arr = ndarray::Array2::from_shape_vec((h, w), img.data)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(arr.into_pyarray_bound(py))
    }

    /// Compute and return image statistics as a Python dict.
    fn stats(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let img = load_image(py, &self.path, self.hdu_index)?;
        let s = compute_stats(&img.data);
        let d = PyDict::new_bound(py);
        d.set_item("npix", s.npix)?;
        d.set_item("n_finite", s.n_finite)?;
        d.set_item("min", s.min)?;
        d.set_item("max", s.max)?;
        d.set_item("mean", s.mean)?;
        d.set_item("median", s.median)?;
        d.set_item("std_dev", s.std_dev)?;
        d.set_item("sum", s.sum)?;
        Ok(d.into())
    }

    /// Return WCS center (RA, Dec) in degrees, or None if no WCS.
    fn wcs_center(&self, py: Python<'_>) -> PyResult<Option<(f64, f64)>> {
        let img = load_image(py, &self.path, self.hdu_index)?;
        let wcs = Wcs::from_header(&img.header);
        Ok(wcs.and_then(|w| w.pixel_to_world(img.width as f64 / 2.0, img.height as f64 / 2.0)))
    }

    fn __repr__(&self) -> String {
        format!("FitsImage('{}', hdu={})", self.path.display(), self.hdu_index)
    }
}

fn load_image(
    py: Python<'_>,
    path: &std::path::Path,
    hdu: usize,
) -> PyResult<fitsview_core::fits_reader::FitsImage> {
    let img = if hdu == 0 {
        load_fits(path)
    } else {
        load_fits_hdu(path, hdu)
    };
    img.map_err(|e| {
        let _ = py;
        PyErr::new::<pyo3::exceptions::PyIOError, _>(e.to_string())
    })
}

/// Open a FITS file and return a FitsImage object.
///
/// Example::
///
///     import fitsview
///     img = fitsview.open("image.fits")
///     data = img.data()        # np.ndarray[float32, 2D]
///     header = img.header()    # dict[str, str]
///     stats = img.stats()      # dict
#[pyfunction]
fn open(path: &str) -> PyResult<PyFitsImage> {
    Ok(PyFitsImage {
        path: std::path::PathBuf::from(path),
        hdu_index: 0,
    })
}

/// Apply ZScale to pixel data and return (vmin, vmax).
#[pyfunction]
fn zscale(data: numpy::PyReadonlyArray2<f32>) -> (f32, f32) {
    let slice = data.as_slice().unwrap_or(&[]);
    let sr = fitsview_core::scale::compute_scale(slice, fitsview_core::scale::ScaleMode::ZScale);
    (sr.vmin, sr.vmax)
}

#[pymodule]
fn fitsview(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyFitsImage>()?;
    m.add_function(wrap_pyfunction!(open, m)?)?;
    m.add_function(wrap_pyfunction!(zscale, m)?)?;
    Ok(())
}
