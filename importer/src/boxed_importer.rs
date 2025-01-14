use crate::error::Result;
use crate::{AssetTypeId, AssetUuid, Importer, ImporterValue, SerdeObj};
use ron;
use serde::{Deserialize, Serialize};
use std::io::Read;
use type_uuid::{TypeUuid, TypeUuidDynamic};

/// Serializable metadata for an asset.
/// Stored in .meta files and metadata DB.
#[derive(Clone, Serialize, Deserialize, Hash, Default)]
pub struct AssetMetadata {
    /// UUID for the asset to uniquely identify it
    pub id: AssetUuid,
    /// Search tags are used by asset tooling to search for the imported asset
    pub search_tags: Vec<(String, Option<String>)>,
    /// Build dependencies will be included in the Builder arguments when building the asset
    pub build_deps: Vec<AssetUuid>,
    /// Load dependencies are guaranteed to load before this asset by the Loader
    pub load_deps: Vec<AssetUuid>,
    /// Instantiate dependencies will be instantiated along with this asset when
    /// the asset is instantiated into a world. Only applies for Prefabs.
    pub instantiate_deps: Vec<AssetUuid>,
    /// The referenced build pipeline is invoked when a build artifact is requested for the imported asset
    pub build_pipeline: Option<AssetUuid>,
    /// The UUID of the asset's Rust type
    pub import_asset_type: AssetTypeId,
}
/// Version of the SourceMetadata struct.
/// Used for forward compatibility to enable changing the .meta file format
pub const SOURCEMETADATA_VERSION: u32 = 1;

/// SourceMetadata is the in-memory representation of the .meta file for a (source, .meta) pair.
#[derive(Serialize, Deserialize)]
pub struct SourceMetadata<Options, State> {
    /// Metadata struct version
    pub version: u32,
    /// Hash of the source file + importer options + importer state when last importing source file.
    pub import_hash: Option<u64>,
    /// The [Importer::version] used to import the source file.
    pub importer_version: u32,
    /// The [type_uuid::TypeUuid::UUID] used to import the source file.
    #[serde(default)]
    pub importer_type: AssetTypeId,
    /// The [Importer::Options] used to import the source file.
    pub importer_options: Options,
    /// The [Importer::State] generated when importing the source file.
    pub importer_state: State,
    /// Metadata for assets generated when importing the source file.
    pub assets: Vec<AssetMetadata>,
}

/// Trait object wrapper for [Importer] implementations.
/// Enables using Importers without knowing the concrete type.
/// See [Importer] for documentation on fields.
pub trait BoxedImporter: TypeUuidDynamic + Send + Sync {
    fn import_boxed(
        &self,
        source: &mut dyn Read,
        options: Box<dyn SerdeObj>,
        state: Box<dyn SerdeObj>,
    ) -> Result<BoxedImporterValue>;
    fn default_options(&self) -> Box<dyn SerdeObj>;
    fn default_state(&self) -> Box<dyn SerdeObj>;
    fn version(&self) -> u32;
    fn deserialize_metadata<'a>(
        &self,
        bytes: &'a [u8],
    ) -> Result<SourceMetadata<Box<dyn SerdeObj>, Box<dyn SerdeObj>>>;
    fn deserialize_options<'a>(&self, bytes: &'a [u8]) -> Result<Box<dyn SerdeObj>>;
    fn deserialize_state<'a>(&self, bytes: &'a [u8]) -> Result<Box<dyn SerdeObj>>;
}
/// Trait object wrapper for [ImporterValue] implementations.
/// See [ImporterValue] for documentation on fields.
pub struct BoxedImporterValue {
    pub value: ImporterValue,
    pub options: Box<dyn SerdeObj>,
    pub state: Box<dyn SerdeObj>,
}

impl<S, O, T> BoxedImporter for T
where
    O: SerdeObj + Serialize + Default + Send + Sync + Clone + for<'a> Deserialize<'a>,
    S: SerdeObj + Serialize + Default + Send + Sync + for<'a> Deserialize<'a>,
    T: Importer<State = S, Options = O> + TypeUuid + Send + Sync,
{
    fn import_boxed(
        &self,
        source: &mut dyn Read,
        options: Box<dyn SerdeObj>,
        state: Box<dyn SerdeObj>,
    ) -> Result<BoxedImporterValue> {
        let mut s = state.downcast::<S>().unwrap();
        let o = *options.downcast::<O>().unwrap();
        let result = self.import(source, o.clone(), &mut s)?;
        Ok(BoxedImporterValue {
            value: result,
            options: Box::new(o),
            state: s,
        })
    }
    fn default_options(&self) -> Box<dyn SerdeObj> {
        Box::new(O::default())
    }
    fn default_state(&self) -> Box<dyn SerdeObj> {
        Box::new(S::default())
    }
    fn version(&self) -> u32 {
        self.version()
    }
    fn deserialize_metadata<'a>(
        &self,
        bytes: &'a [u8],
    ) -> Result<SourceMetadata<Box<dyn SerdeObj>, Box<dyn SerdeObj>>> {
        let metadata: SourceMetadata<O, S> = ron::de::from_bytes(&bytes)?;
        Ok(SourceMetadata {
            version: metadata.version,
            import_hash: metadata.import_hash,
            importer_version: metadata.importer_version,
            importer_type: metadata.importer_type,
            importer_options: Box::new(metadata.importer_options),
            importer_state: Box::new(metadata.importer_state),
            assets: metadata.assets.clone(),
        })
    }
    fn deserialize_options<'a>(&self, bytes: &'a [u8]) -> Result<Box<dyn SerdeObj>> {
        Ok(Box::new(bincode::deserialize::<O>(&bytes)?))
    }
    fn deserialize_state<'a>(&self, bytes: &'a [u8]) -> Result<Box<dyn SerdeObj>> {
        Ok(Box::new(bincode::deserialize::<S>(&bytes)?))
    }
}

/// Use [inventory::submit!] to register an importer to use for a file extension.
#[derive(Debug)]
pub struct SourceFileImporter {
    pub extension: &'static str,
    pub instantiator: fn() -> Box<dyn BoxedImporter>,
}
inventory::collect!(SourceFileImporter);

/// Get the registered importers and their associated extension.
pub fn get_source_importers(
) -> impl Iterator<Item = (&'static str, Box<dyn BoxedImporter + 'static>)> {
    inventory::iter::<SourceFileImporter>
        .into_iter()
        .map(|s| (s.extension.trim_start_matches("."), (s.instantiator)()))
}
