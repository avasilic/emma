use h3o::{CellIndex, LatLng, Resolution};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::BufRead;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionInfo {
    pub country: String,
    pub region: String,
    pub timezone: String,
    pub nearest_place: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationResult {
    pub country: String,
    pub region: String,
    pub timezone: String,
    pub nearest_place: String,
    pub h3_cells: [u64; 9], // H3 cell IDs for resolutions 0-8
    pub resolution_used: u8,
}

pub struct H3Geocoder {
    region_maps: [HashMap<u64, RegionInfo>; 9],
}

impl H3Geocoder {
    pub fn from_geonames_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);

        let mut region_maps: [HashMap<u64, RegionInfo>; 9] = Default::default();

        println!("Building H3 spatial index for resolutions 0-8...");

        for (line_num, line) in reader.lines().enumerate() {
            let line = line?;
            let fields: Vec<&str> = line.split('\t').collect();

            if fields.len() >= 18 {
                if let (Ok(lat), Ok(lng)) = (fields[4].parse::<f64>(), fields[5].parse::<f64>()) {
                    // Create LatLng coordinate (MUST ensure valid)
                    if let Ok(coord) = LatLng::new(lat, lng) {
                        let region_info = RegionInfo {
                            country: fields[8].to_string(),
                            region: fields[10].to_string(),
                            timezone: fields[17].to_string(),
                            nearest_place: fields[1].to_string(),
                        };

                        // Build cells for all resolutions 0-8
                        for res in 0..=8u8 {
                            let resolution = Resolution::try_from(res)?;
                            let cell = coord.to_cell(resolution);
                            let cell_id: u64 = cell.into();

                            // Only insert if we don't have this cell yet
                            region_maps[res as usize]
                                .entry(cell_id)
                                .or_insert(region_info.clone());
                        }
                    }
                }
            }

            if line_num % 100_000 == 0 {
                println!("Processed {line_num} locations...");
            }
        }

        // Print stats
        for (res, map) in region_maps.iter().enumerate() {
            println!("Resolution {}: {} unique cells", res, map.len());
        }

        Ok(Self { region_maps })
    }

    /// Get everything at once: country, region, timezone, and all H3 cell IDs
    pub fn get_complete_location_info(&self, lat: f64, lng: f64) -> Option<LocationResult> {
        let coord = LatLng::new(lat, lng).ok()?;

        // Compute H3 cells for all resolutions
        let mut h3_cells = [0u64; 9];

        for res in 0..=8u8 {
            if let Ok(resolution) = Resolution::try_from(res) {
                let cell = coord.to_cell(resolution);
                h3_cells[res as usize] = cell.into();
            }
        }

        // Find region info (try highest resolution first)
        for res in (0..=8usize).rev() {
            let cell_id = h3_cells[res];
            if let Some(region_info) = self.region_maps[res].get(&cell_id) {
                return Some(LocationResult {
                    country: region_info.country.clone(),
                    region: region_info.region.clone(),
                    timezone: region_info.timezone.clone(),
                    nearest_place: region_info.nearest_place.clone(),
                    h3_cells,
                    resolution_used: res as u8,
                });
            }
        }

        None
    }

    /// Get just the H3 cell ID for a specific resolution
    pub fn get_cell_id(&self, lat: f64, lng: f64, resolution: u8) -> Option<u64> {
        if resolution > 8 {
            return None;
        }

        let coord = LatLng::new(lat, lng).ok()?;
        let res = Resolution::try_from(resolution).ok()?;
        let cell = coord.to_cell(res);
        Some(cell.into())
    }

    /// Convert H3 cell back to center coordinates
    pub fn cell_to_center(&self, cell_id: u64) -> Option<(f64, f64)> {
        if let Ok(cell) = CellIndex::try_from(cell_id) {
            let center: LatLng = cell.into();
            Some((center.lat(), center.lng()))
        } else {
            None
        }
    }
}
