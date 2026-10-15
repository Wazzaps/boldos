use alloc::vec;
use zerocopy::FromBytes;

pub trait ReadAt {
    fn read_at(&self, buf: &mut [u8], offset: u64) -> usize;
}

impl<T: ReadAt + ?Sized> ReadAt for &T {
    fn read_at(&self, buf: &mut [u8], offset: u64) -> usize {
        (*self).read_at(buf, offset)
    }
}

impl ReadAt for &[u8] {
    fn read_at(&self, buf: &mut [u8], offset: u64) -> usize {
        let offset = offset as usize;
        let len = self.len().min(buf.len());
        buf[..len].copy_from_slice(&self[offset..offset + len]);
        len
    }
}

pub trait ReadAtZerocopyExt: Sized {
    fn read_from_bytes_at<R: ReadAt>(source: R, offset: u64) -> Result<Self, ()>;
}

impl<T: FromBytes + Sized> ReadAtZerocopyExt for T {
    fn read_from_bytes_at<R: ReadAt>(source: R, offset: u64) -> Result<Self, ()> {
        let mut buf = vec![0u8; size_of::<T>()];
        if source.read_at(&mut buf, offset) != size_of::<T>() {
            return Err(());
        }
        T::read_from_bytes(&buf).map_err(|_| ())
    }
}
